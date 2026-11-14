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

use std::cmp::Reverse;
use std::collections::{BinaryHeap, BTreeMap, HashMap};

use openflex_model::{
    Action, Connection, Distribution, EvalContext, Model, Object, ObjectKind, ObjectParams,
    PortStrategy, QueueOrder, SimTrigger, Value,
};
use rand::prelude::*;
use rand_distr::{Distribution as _, Exp, Gamma, LogNormal, Normal};

use crate::error::SimError;
use crate::stats::{ObjectStats, ReplicationResult, SimConfig, SimReport, StateTimes};

/// Safety valve: a model that never drains its events is reported instead of
/// hanging forever.
const MAX_EVENTS: u64 = 50_000_000;

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
    SetupDone(usize),
    ProcessDone(usize),
    Breakdown(usize),
    RepairDone(usize),
    /// A user message reaches an object.
    Message(usize),
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

struct ObjectRt {
    name: String,
    kind: ObjectKind,
    content: Vec<FlowItem>,
    /// Items that finished processing but could not be delivered yet.
    out_pending: Vec<FlowItem>,
    state: State,
    phase: Phase,
    last_state_change: f64,
    last_content_change: f64,
    acc: Accum,
    rr: usize,
    /// Source bookkeeping.
    arrivals: u32,
    /// Processor bookkeeping.
    processing_item: Option<FlowItem>,
}

impl ObjectRt {
    fn new(name: String, kind: ObjectKind, now: f64) -> Self {
        Self {
            name,
            kind,
            content: Vec::new(),
            out_pending: Vec::new(),
            state: State::Idle,
            phase: Phase::Idle,
            last_state_change: now,
            last_content_change: now,
            acc: Accum::default(),
            rr: 0,
            arrivals: 0,
            processing_item: None,
        }
    }

    fn note_content(&mut self, now: f64) {
        let dt = (now - self.last_content_change).max(0.0);
        if self.kind == ObjectKind::Processor && dt > 1.0 {
            eprintln!("note {} at {now} dt={dt} len={} proc={} integral={}", self.name, self.content.len(), self.processing_item.is_some(), self.acc.content_integral);
        }
        self.acc.content_integral += self.content.len() as f64 * dt;
        self.last_content_change = now;
        self.acc.max_content = self.acc.max_content.max(self.content.len() as u32);
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
    let items_completed: u64 = sim
        .objects
        .iter()
        .filter(|object| object.kind == ObjectKind::Sink)
        .map(|object| object.acc.output)
        .sum();
    let items_in_system: u64 = sim
        .objects
        .iter()
        .map(|object| {
            object.content.len() as u64
                + u64::from(object.processing_item.is_some())
                + object.out_pending.len() as u64
        })
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
    rng: StdRng,
    items_created: u64,
    /// Items created since the last warm-up reset (reported).
    stats_created: u64,
    items_completed: u64,
    event_count: u64,
}

impl<'m> Simulation<'m> {
    fn new(model: &'m Model, config: SimConfig) -> Result<Self, SimError> {
        let mut by_name = HashMap::new();
        let mut objects = Vec::with_capacity(model.objects.len());
        for (index, object) in model.objects.iter().enumerate() {
            if by_name.insert(object.name.clone(), index).is_some() {
                return Err(SimError::Model(openflex_model::ModelError::Invalid(vec![
                    openflex_model::ValidationIssue::new(
                        format!("objects.{}", object.name),
                        "duplicate object name",
                    ),
                ])));
            }
            objects.push(ObjectRt::new(object.name.clone(), object.kind, 0.0));
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
            rng: StdRng::seed_from_u64(seed),
            items_created: 0,
            stats_created: 0,
            items_completed: 0,
            event_count: 0,
        };
        sim.initialize()?;
        Ok(sim)
    }

    fn initialize(&mut self) -> Result<(), SimError> {
        if self.config.warmup > 0.0 {
            self.schedule(self.config.warmup, EventKind::ResetStats);
        }
        for index in 0..self.model.objects.len() {
            let params = self.model.objects[index].params.clone();
            match params {
                ObjectParams::Source(params) => {
                    let first = if params.first_arrival > 0.0 {
                        params.first_arrival
                    } else {
                        self.sample(&params.interarrival)
                    };
                    self.schedule(first, EventKind::Arrive(index));
                }
                ObjectParams::Processor(params) => {
                    if let Some(failure) = &params.failure {
                        let mtbf = self.sample(&failure.mtbf).max(1e-9);
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
        match distribution {
            Distribution::Constant { value } => *value,
            Distribution::Uniform { min, max } => self.rng.r#gen_range(*min..*max),
            Distribution::Normal { mean, stddev } => {
                if *stddev <= 0.0 {
                    *mean
                } else {
                    Normal::new(*mean, *stddev).map(|d| d.sample(&mut self.rng)).unwrap_or(*mean)
                }
            }
            Distribution::Exponential { mean } => {
                Exp::new(1.0 / *mean).map(|d| d.sample(&mut self.rng)).unwrap_or(*mean)
            }
            Distribution::Triangular { min, mode, max } => {
                let u: f64 = self.rng.r#gen();
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
            Distribution::Gamma { shape, scale } => {
                Gamma::new(*shape, *scale).map(|d| d.sample(&mut self.rng)).unwrap_or(*scale)
            }
            Distribution::Weibull { shape, scale } => {
                // Inverse CDF: scale * (-ln(U))^(1/shape)
                let u: f64 = self.rng.r#gen::<f64>().max(1e-12);
                scale * (-u.ln()).powf(1.0 / shape)
            }
            Distribution::Lognormal { meanlog, sdlog } => {
                if *sdlog <= 0.0 {
                    meanlog.exp()
                } else {
                    LogNormal::new(*meanlog, *sdlog)
                        .map(|d| d.sample(&mut self.rng))
                        .unwrap_or_else(|_| meanlog.exp())
                }
            }
            Distribution::Erlang { shape, scale } => {
                let mut total = 0.0;
                for _ in 0..*shape {
                    let u: f64 = self.rng.r#gen::<f64>().max(1e-12);
                    total += -scale * u.ln();
                }
                total
            }
            Distribution::Beta { alpha, beta } => {
                // Two gamma samples: X ~ Gamma(alpha), Y ~ Gamma(beta), X/(X+Y).
                let x = Gamma::new(*alpha, 1.0).map(|d| d.sample(&mut self.rng)).unwrap_or(0.0);
                let y = Gamma::new(*beta, 1.0).map(|d| d.sample(&mut self.rng)).unwrap_or(1.0);
                if x + y > 0.0 { x / (x + y) } else { 0.5 }
            }
            Distribution::IntegerUniform { min, max } => self.rng.r#gen_range(*min..=*max) as f64,
            Distribution::Empirical { values, weights } => {
                if values.is_empty() {
                    return 0.0;
                }
                match weights {
                    None => values[self.rng.r#gen_range(0..values.len())],
                    Some(weights) if weights.len() == values.len() => {
                        let total: f64 = weights.iter().sum();
                        let mut pick = self.rng.r#gen::<f64>() * total;
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
            EventKind::SetupDone(index) => self.handle_setup_done(index),
            EventKind::ProcessDone(index) => self.handle_process_done(index),
            EventKind::Breakdown(index) => self.handle_breakdown(index),
            EventKind::RepairDone(index) => self.handle_repair(index),
            EventKind::Message(index) => self.handle_message(index),
            EventKind::ResetStats => {
                self.reset_stats();
                Ok(())
            }
        }
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
            && self.now > stop
        {
            return Ok(());
        }
        self.objects[index].arrivals += 1;
        for _ in 0..params.items_per_arrival {
            let item = self.create_item(params.item_type, &params.labels, index)?;
            self.objects[index].out_pending.push(item);
        }
        self.flow(index)?;
        if params.max_arrivals.is_none_or(|max| self.objects[index].arrivals < max) {
            let next = self.now + self.sample(&params.interarrival);
            if next <= self.config.stop_time {
                self.schedule(next, EventKind::Arrive(index));
            }
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

    // -----------------------------------------------------------------------
    // Flow, routing and delivery
    // -----------------------------------------------------------------------

    /// Try to push everything an object can currently send.
    fn flow(&mut self, index: usize) -> Result<(), SimError> {
        let kind = self.model.objects[index].kind;
        // Deliver items that finished processing earlier.
        while let Some(item) = self.objects[index].out_pending.first().cloned() {
            match self.route(index, &item)? {
                Some(target) => {
                    if self.deliver(index, target, item.clone())? {
                        self.objects[index].out_pending.remove(0);
                        self.objects[index].acc.output += 1;
                        self.objects[index].acc.stay_sum += self.now - item.entry_time;
                        self.objects[index].acc.stay_count += 1;
                    } else {
                        self.objects[index].set_state(self.now, State::Blocked);
                        self.objects[index].phase = Phase::WaitingForOutput;
                        return Ok(());
                    }
                }
                None => {
                    self.objects[index].set_state(self.now, State::Blocked);
                    self.objects[index].phase = Phase::WaitingForOutput;
                    return Ok(());
                }
            }
        }

        match kind {
            ObjectKind::Queue | ObjectKind::Rack => {
                self.flow_queue(index)?;
            }
            ObjectKind::Processor => {
                self.flow_processor(index)?;
            }
            ObjectKind::Source | ObjectKind::Conveyor => {}
            _ => {
                self.flow_generic(index)?;
            }
        }

        if self.objects[index].out_pending.is_empty()
            && self.objects[index].content.is_empty()
            && !matches!(self.objects[index].phase, Phase::Setup | Phase::Processing)
            && self.objects[index].state == State::Idle
        {
            self.objects[index].phase = Phase::Idle;
        }
        Ok(())
    }

    fn flow_queue(&mut self, index: usize) -> Result<(), SimError> {
        let params = queue_params_of(&self.model.objects[index]);
        loop {
            if self.objects[index].content.is_empty() {
                self.objects[index].set_state(self.now, State::Idle);
                return Ok(());
            }
            if let Some(batch) = params.batch_size
                && self.objects[index].content.len() < batch as usize
            {
                self.objects[index].set_state(self.now, State::Blocked);
                return Ok(());
            }
            let position = self.pick_queue_position(index, &params.order);
            let item = self.objects[index].content.remove(position);
            match self.route(index, &item)? {
                Some(target) => {
                    if self.deliver(index, target, item.clone())? {
                        self.objects[index].acc.output += 1;
                        self.objects[index].acc.stay_sum += self.now - item.entry_time;
                        self.objects[index].acc.stay_count += 1;
                        self.objects[index].note_content(self.now);
                    } else {
                        let at = position.min(self.objects[index].content.len());
                        self.objects[index].content.insert(at, item);
                        self.objects[index].set_state(self.now, State::Blocked);
                        return Ok(());
                    }
                }
                None => {
                    let at = position.min(self.objects[index].content.len());
                    self.objects[index].content.insert(at, item);
                    self.objects[index].set_state(self.now, State::Blocked);
                    return Ok(());
                }
            }
        }
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

    fn flow_processor(&mut self, index: usize) -> Result<(), SimError> {
        let params = match &self.model.objects[index].params {
            ObjectParams::Processor(params) => params.clone(),
            _ => return Ok(()),
        };
        if matches!(self.objects[index].phase, Phase::Setup | Phase::Processing) {
            return Ok(());
        }
        if self.objects[index].processing_item.is_some() {
            return Ok(());
        }
        if self.objects[index].state == State::Down {
            return Ok(());
        }
        if self.objects[index].content.is_empty() {
            self.objects[index].set_state(self.now, State::Idle);
            self.objects[index].phase = Phase::Idle;
            return Ok(());
        }
        // Start the next item.
        let item = self.objects[index].content.remove(0);
        self.objects[index].note_content(self.now);
        self.objects[index].processing_item = Some(item);
        self.objects[index].set_state(self.now, State::Busy);
        let setup = self.sample(&params.setup_time).max(0.0);
        if setup > 0.0 {
            self.objects[index].phase = Phase::Setup;
            self.schedule(self.now + setup, EventKind::SetupDone(index));
        } else {
            let process = self.sample(&params.process_time).max(0.0);
            self.objects[index].phase = Phase::Processing;
            self.schedule(self.now + process, EventKind::ProcessDone(index));
        }
        Ok(())
    }

    fn handle_setup_done(&mut self, index: usize) -> Result<(), SimError> {
        let params = match &self.model.objects[index].params {
            ObjectParams::Processor(params) => params.clone(),
            _ => return Ok(()),
        };
        let process = self.sample(&params.process_time).max(0.0);
        self.objects[index].phase = Phase::Processing;
        self.schedule(self.now + process, EventKind::ProcessDone(index));
        Ok(())
    }

    fn handle_process_done(&mut self, index: usize) -> Result<(), SimError> {
        let Some(item) = self.objects[index].processing_item.take() else {
            self.objects[index].phase = Phase::Idle;
            self.objects[index].set_state(self.now, State::Idle);
            return Ok(());
        };
        self.objects[index].out_pending.push(item);
        self.objects[index].phase = Phase::WaitingForOutput;
        // `flow` delivers the finished item and, when there is another item
        // waiting, starts the next processing step.
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

    /// Objects without a dedicated behaviour pass items through, which keeps
    /// unknown kinds usable while the specialised behaviour is added.
    fn flow_generic(&mut self, index: usize) -> Result<(), SimError> {
        while let Some(item) = self.objects[index].content.first().cloned() {
            match self.route(index, &item)? {
                Some(target) => {
                    if self.deliver(index, target, item.clone())? {
                        self.objects[index].content.remove(0);
                        self.objects[index].acc.output += 1;
                        self.objects[index].acc.stay_sum += self.now - item.entry_time;
                        self.objects[index].acc.stay_count += 1;
                        self.objects[index].note_content(self.now);
                    } else {
                        self.objects[index].set_state(self.now, State::Blocked);
                        return Ok(());
                    }
                }
                None => {
                    self.objects[index].set_state(self.now, State::Blocked);
                    return Ok(());
                }
            }
        }
        self.objects[index].set_state(self.now, State::Idle);
        Ok(())
    }

    fn route(&mut self, from: usize, item: &FlowItem) -> Result<Option<usize>, SimError> {
        let name = self.model.objects[from].name.clone();
        let outputs: Vec<&Connection> = self.model.outputs(&name);
        if outputs.is_empty() {
            return Ok(None);
        }
        let strategy = outputs
            .iter()
            .find_map(|connection| connection.strategy.clone())
            .unwrap_or(PortStrategy::FirstAvailable);
        let target = match strategy {
            PortStrategy::FirstAvailable => outputs
                .iter()
                .find(|connection| self.can_accept_name(&connection.to))
                .and_then(|connection| self.by_name.get(&connection.to).copied()),
            PortStrategy::RoundRobin => {
                let start = self.objects[from].rr % outputs.len();
                let found = (0..outputs.len())
                    .map(|offset| (start + offset) % outputs.len())
                    .find(|&offset| self.can_accept_name(&outputs[offset].to))
                    .map(|offset| outputs[offset].to.clone());
                self.objects[from].rr = self.objects[from].rr.wrapping_add(1);
                found.and_then(|name| self.by_name.get(&name).copied())
            }
            PortStrategy::Random => {
                let available: Vec<&Connection> = outputs
                    .iter()
                    .copied()
                    .filter(|connection| self.can_accept_name(&connection.to))
                    .collect();
                if available.is_empty() {
                    None
                } else {
                    let pick = self.rng.r#gen_range(0..available.len());
                    self.by_name.get(&available[pick].to).copied()
                }
            }
            PortStrategy::ByPercentage { percentages } => {
                let total: f64 = percentages.iter().sum();
                if percentages.len() != outputs.len() || total <= 0.0 {
                    None
                } else {
                    let mut pick = self.rng.r#gen::<f64>() * total;
                    let mut chosen = None;
                    for (connection, weight) in outputs.iter().zip(&percentages) {
                        if pick <= *weight {
                            chosen = Some(connection.to.clone());
                            break;
                        }
                        pick -= *weight;
                    }
                    chosen.and_then(|name| self.by_name.get(&name).copied())
                }
            }
            PortStrategy::ByCondition { conditions } => {
                let mut chosen = None;
                for condition in &conditions {
                    let matches = {
                        let context = ExprContext::new(self, Some(item));
                        condition.condition.eval(&context).map_err(|err| SimError::Expression {
                            location: format!("{name} routing condition"),
                            detail: err.to_string(),
                        })?
                    };
                    if matches.as_bool() {
                        chosen = outputs.get(condition.port as usize).map(|c| c.to.clone());
                        break;
                    }
                }
                chosen.and_then(|name| self.by_name.get(&name).copied())
            }
            PortStrategy::ByItemType => outputs
                .get(item.item_type as usize)
                .and_then(|connection| self.by_name.get(&connection.to).copied()),
            PortStrategy::ByLabel { label } => {
                let port = item.labels.get(&label).copied().unwrap_or(0.0).max(0.0) as usize;
                outputs.get(port).and_then(|connection| self.by_name.get(&connection.to).copied())
            }
        };
        Ok(target)
    }

    fn can_accept_name(&self, name: &str) -> bool {
        match self.by_name.get(name) {
            Some(&index) => self.can_accept(index),
            None => false,
        }
    }

    fn can_accept(&self, index: usize) -> bool {
        let limit = self.capacity_of(index);
        // A processor's item in process counts towards its capacity: an item
        // being processed is still "in" the object.
        let used = self.objects[index].content.len()
            + usize::from(self.objects[index].processing_item.is_some());
        used < limit as usize
    }

    fn capacity_of(&self, index: usize) -> u32 {
        match &self.model.objects[index].params {
            ObjectParams::Source(_) => 0,
            ObjectParams::Queue(params) => params.max_content,
            ObjectParams::Processor(params) => params.max_content,
            ObjectParams::Sink(_) => u32::MAX,
            ObjectParams::Combiner(params) => params.components_per_container.max(1),
            ObjectParams::Separator(_) => u32::MAX,
            ObjectParams::Conveyor(params) => params.max_content,
            ObjectParams::Merge(_) => u32::MAX,
            ObjectParams::Split(_) => u32::MAX,
            ObjectParams::Transporter(params) => params.capacity,
            ObjectParams::Dispatcher(_) => u32::MAX,
            ObjectParams::Resource(params) => params.capacity,
            ObjectParams::Rack(params) => params.capacity,
            ObjectParams::BasicFr(_) => 1,
            ObjectParams::BasicTe(params) => params.capacity,
        }
    }

    /// Deliver an item into `target`. Returns `false` when the target rejects
    /// it; the caller then keeps the item and retries later.
    fn deliver(&mut self, from: usize, target: usize, item: FlowItem) -> Result<bool, SimError> {
        if !self.can_accept(target) {
            return Ok(false);
        }
        let mut item = item;
        let kind = self.model.objects[target].kind;
        // Trigger the receiving object's OnEntry logic.
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
        match kind {
            ObjectKind::Sink => {
                self.items_completed += 1;
                self.objects[target].acc.output += 1;
                self.objects[target].acc.stay_sum += self.now - item.entry_time;
                self.objects[target].acc.stay_count += 1;
                self.trigger_actions_run(target, SimTrigger::OnExit, &mut item)?;
            }
            ObjectKind::Queue | ObjectKind::Rack => {
                self.objects[target].content.push(item);
                self.objects[target].note_content(self.now);
                self.objects[target].set_state(self.now, State::Busy);
                self.schedule(self.now, EventKind::Flow(target));
            }
            ObjectKind::Processor => {
                self.objects[target].content.push(item);
                self.objects[target].note_content(self.now);
                if self.objects[target].phase == Phase::Idle
                    && self.objects[target].state != State::Down
                {
                    self.flow_processor(target)?;
                }
            }
            _ => {
                self.objects[target].content.push(item);
                self.objects[target].note_content(self.now);
                self.schedule(self.now, EventKind::Flow(target));
            }
        }
        let _ = from;
        self.notify_upstreams(target)?;
        Ok(true)
    }

    fn notify_upstreams(&mut self, index: usize) -> Result<(), SimError> {
        if !self.can_accept(index) {
            return Ok(());
        }
        let name = self.model.objects[index].name.clone();
        for connection in self.model.inputs(&name) {
            if let Some(&from) = self.by_name.get(&connection.from) {
                self.schedule(self.now, EventKind::Flow(from));
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Failures
    // -----------------------------------------------------------------------

    fn handle_breakdown(&mut self, index: usize) -> Result<(), SimError> {
        let params = match &self.model.objects[index].params {
            ObjectParams::Processor(params) => params.clone(),
            _ => return Ok(()),
        };
        let Some(failure) = &params.failure else { return Ok(()) };
        if self.objects[index].state == State::Down {
            return Ok(());
        }
        self.objects[index].set_state(self.now, State::Down);
        let repair = self.sample(&failure.mttr).max(0.0);
        self.schedule(self.now + repair, EventKind::RepairDone(index));
        Ok(())
    }

    fn handle_repair(&mut self, index: usize) -> Result<(), SimError> {
        let params = match &self.model.objects[index].params {
            ObjectParams::Processor(params) => params.clone(),
            _ => return Ok(()),
        };
        self.objects[index].set_state(self.now, State::Idle);
        self.objects[index].phase = Phase::Idle;
        if let Some(failure) = &params.failure {
            let mtbf = self.sample(&failure.mtbf).max(1e-9);
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
        let mut probe = FlowItem {
            id: 0,
            item_type: 0,
            labels: BTreeMap::new(),
            created: self.now,
            entry_time: self.now,
        };
        self.apply_actions(index, &mut probe, actions)
    }

    // -----------------------------------------------------------------------
    // Triggers
    // -----------------------------------------------------------------------

    fn trigger_actions(&self, index: usize, trigger: SimTrigger) -> Vec<Action> {
        let params = &self.model.objects[index].params;
        let triggers = params.triggers();
        triggers.actions(trigger)
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
        self.items_completed = 0;
        for object in &mut self.objects {
            object.acc = Accum::default();
            object.last_state_change = self.now;
            object.last_content_change = self.now;
        }
    }
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
                return Some(Value::Number(self.sim.items_created as f64));
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
        if let Some(rest) = path.strip_prefix("object.") {
            let (name, field) = rest.split_once('.')?;
            let index = *self.sim.by_name.get(name)?;
            return object_field(self.sim, index, field);
        }
        if let Some(rest) = path.strip_prefix("var.") {
            return self
                .item
                .and_then(|item| item.labels.get(rest).copied())
                .map(Value::Number);
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
        "content" | "wip" => Some(Value::Number(object.content.len() as f64)),
        "state" => Some(Value::Str(object.state.name().to_string())),
        "input" | "inputs" => Some(Value::Number(object.acc.input as f64)),
        "output" | "outputs" => Some(Value::Number(object.acc.output as f64)),
        "idle" => Some(Value::Number(object.acc.states.idle)),
        "busy" => Some(Value::Number(object.acc.states.busy)),
        "blocked" => Some(Value::Number(object.acc.states.blocked)),
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
