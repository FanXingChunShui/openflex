//! Static catalogs shared by the CLI commands and the `serve` endpoint:
//! object kinds, their parameters and the statistical distributions.

use openflex_model::ObjectKind;
use serde_json::{Value, json};

/// One-line description and parameter help for an object kind.
pub fn describe_kind(kind: ObjectKind) -> (&'static str, &'static str) {
    match kind {
        ObjectKind::Source => (
            "Creates flow items and releases them to the next object.",
            "arrival_style: inter_arrival_time | arrival_schedule | arrival_sequence (inferred from the table fields)\ninterarrival: distribution (default: constant 0; only in inter_arrival_time mode)\nitems_per_arrival: integer, default 1\nmax_arrivals: optional integer\nfirst_arrival: time of the first arrival, default 0\nstop_time: optional time after which no items are created\nitem_type: integer label for new items, default 0\narrival_schedule: { rows: [{ time, quantity, item_type?, labels? }], repeat } (absolute arrival times)\narrival_sequence: [{ quantity, item_type?, labels? }] (rows advance when the previous batch has left)\nlabels: map of label name to number\npull: { enabled, strategy, requirement } (A3 pull mode)\ntriggers: on_create, on_entry, on_exit, on_receive, on_message; actions include release_item",
        ),
        ObjectKind::Queue => (
            "Stores flow items until the next object can take them.",
            "max_content: capacity, default 1000\norder: fifo | lifo | priority | by_label(<name>)\nbatch_size: optional integer (release items in batches)\nbatch_max_wait: time, default 0 (wait forever)\npull: { enabled, strategy, requirement }",
        ),
        ObjectKind::Processor => (
            "Processes one or more flow items with setup and process times.",
            "setup_time: distribution, default constant 0\nprocess_time: distribution, default constant 1\nmax_content: capacity including the item in process, default 1\nchangeover_time: distribution, default constant 0\nfailure: { mtbf, mttr, first_failure_time?, count_time_in_states? } optional failure/repair cycle\nsetup_operator / process_operator: dispatcher names\nnum_setup_operators / num_process_operators: integers (default 1)\nuse_same_num_operators: boolean, default false\noperator_priority: integer, default 0\noperator_preempt: boolean, default false\npull: { enabled, strategy, requirement }\ntriggers: object triggers and actions (release_item supported)",
        ),
        ObjectKind::MultiProcessor => (
            "Processes several flow items in parallel through independent slots.",
            "slots: number of parallel processing slots, default 1\nsetup_time: distribution per slot, default constant 0\nprocess_time: distribution per slot, default constant 1\nsetup_operator / process_operator: dispatcher names (crew of `slots` requested per busy period)",
        ),
        ObjectKind::Sink => (
            "Removes finished flow items and counts them.",
            "pull: { enabled, strategy, requirement }",
        ),
        ObjectKind::Combiner => (
            "Combines components into one container or joins items together.",
            "components_per_container: integer, default 2\nmode: pack | join | batch\nsetup_time / process_time: distributions (batch mode)\nsetup_operator / process_operator: dispatcher names\npull: { enabled, strategy, requirement }",
        ),
        ObjectKind::Separator => (
            "Splits a container or a batch into parts.",
            "split_count: integer, 0 = split into all parts (in split mode: total outputs)\nseparate_mode: unpack | split\npull: { enabled, strategy, requirement }",
        ),
        ObjectKind::Conveyor => (
            "Moves items along a belt with a fixed speed.",
            "speed: distance per time unit, default 1\nlength: default 10\naccumulating: boolean, default false\nitem_spacing: default 1\nmax_content: default 1000",
        ),
        ObjectKind::Merge => (
            "Merges several input flows into one output flow.",
            "wait_for_all: boolean, default false",
        ),
        ObjectKind::Split => (
            "Splits one input flow over several outputs.",
            "by_item_type: boolean, default false (round robin otherwise)",
        ),
        ObjectKind::Transporter => (
            "Carries flow items between objects (task executer).",
            "capacity: integer, default 1\nspeed: distance per time unit, default 1\nload_time / unload_time: distributions\nhome: optional object name",
        ),
        ObjectKind::Dispatcher => (
            "Hands out transport tasks to transporters.",
            "max_tasks: integer, 0 = unlimited",
        ),
        ObjectKind::Resource => (
            "A pool of operators or equipment that can be reserved.",
            "capacity: integer, default 1\ndowntimes: list of { start, end } windows",
        ),
        ObjectKind::Rack => (
            "Stores items in levels (a specialised queue).",
            "capacity: integer, default 100\nlevels: integer, default 1",
        ),
        ObjectKind::BasicFr => (
            "A generic fixed resource.",
            "pull: { enabled, strategy, requirement }",
        ),
        ObjectKind::BasicTe => (
            "A generic task executer.",
            "speed: distance per time unit, default 1\ncapacity: integer, default 1",
        ),
    }
}

/// `GET /objects` payload: every object kind with its description and parameters.
pub fn objects_json() -> Value {
    let objects: Vec<Value> = ObjectKind::all()
        .iter()
        .map(|kind| {
            let (summary, parameters) = describe_kind(*kind);
            json!({
                "kind": kind.as_str(),
                "summary": summary,
                "parameters": parameters,
            })
        })
        .collect();
    json!({ "count": objects.len(), "objects": objects })
}

/// `openflex distributions` rows: name and parameter list.
///
/// The names come from `openflex_model::distribution::DISTRIBUTION_KINDS` so the
/// CLI can never advertise a distribution the model cannot parse.
pub fn distributions() -> Vec<(&'static str, &'static str)> {
    openflex_model::distribution::DISTRIBUTION_KINDS
        .iter()
        .map(|kind| (*kind, parameters(kind)))
        .collect()
}

fn parameters(kind: &str) -> &'static str {
    match kind {
        "constant" => "value",
        "uniform" => "min, max",
        "normal" => "mean, stddev",
        "exponential" => "mean",
        "triangular" => "min, mode, max",
        "gamma" => "shape, scale",
        "weibull" => "shape, scale",
        "lognormal" => "meanlog, sdlog",
        "erlang" => "shape, scale",
        "beta" => "alpha, beta",
        "integer_uniform" => "min, max",
        "empirical" => "values, weights (optional)",
        "pareto" => "shape, scale",
        "log_logistic" => "alpha, beta",
        "rayleigh" => "sigma",
        "binomial" => "n, p",
        "poisson" => "mean",
        "geometric" => "p",
        "negative_binomial" => "r, p",
        "bernoulli" => "p",
        other => {
            let _ = other;
            "see the model documentation"
        }
    }
}
