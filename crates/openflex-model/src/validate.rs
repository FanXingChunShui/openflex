//! Model validation.
//!
//! Every problem that can be detected without running the simulation is
//! reported here, with the location of the offending object. `openflex
//! validate` prints this list, so a model never fails halfway through a run
//! because of a typo.

use crate::model::{
    ArrivalStyle, CombinerMode, Connection, Model, ModelError, Object, ObjectKind, PortStrategy,
    SeparateMode, ValidationIssue,
};
use crate::model::{FORMAT_NAME, FORMAT_VERSION};

pub fn validate(model: &Model) -> Result<(), ModelError> {
    let issues = validate_collect(model);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ModelError::Invalid(issues))
    }
}

/// Collect every validation issue instead of stopping at the first one.
pub fn validate_collect(model: &Model) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    if model.format != FORMAT_NAME {
        issues.push(ValidationIssue::new(
            "format",
            format!("expected \"{FORMAT_NAME}\", found \"{}\"", model.format),
        ));
    }
    if model.version > FORMAT_VERSION {
        issues.push(ValidationIssue::new(
            "version",
            format!(
                "model version {} is newer than this build supports ({FORMAT_VERSION})",
                model.version
            ),
        ));
    }
    if model.name.trim().is_empty() {
        issues.push(ValidationIssue::new("name", "the model needs a name"));
    }
    if model.objects.is_empty() {
        issues.push(ValidationIssue::new("objects", "the model has no objects"));
    }
    if model.run.stop_time <= 0.0 {
        issues.push(ValidationIssue::new(
            "run.stop_time",
            "stop_time must be positive",
        ));
    }
    if model.run.warmup < 0.0 {
        issues.push(ValidationIssue::new(
            "run.warmup",
            "warmup cannot be negative",
        ));
    }
    if model.run.warmup >= model.run.stop_time {
        issues.push(ValidationIssue::new(
            "run.warmup",
            "warmup must be shorter than stop_time",
        ));
    }
    if model.run.replications == 0 {
        issues.push(ValidationIssue::new(
            "run.replications",
            "replications must be at least 1",
        ));
    }

    let mut seen = std::collections::BTreeMap::new();
    for object in &model.objects {
        if !is_identifier(&object.name) {
            issues.push(ValidationIssue::new(
                format!("objects.{}", object.name),
                "object names must start with a letter or underscore and contain only letters, digits and underscores",
            ));
        }
        if seen.insert(object.name.clone(), ()).is_some() {
            issues.push(ValidationIssue::new(
                format!("objects.{}", object.name),
                "duplicate object name",
            ));
        }
        if !object.pos.iter().all(|value| value.is_finite()) {
            issues.push(ValidationIssue::new(
                format!("objects.{}", object.name),
                "position must be finite",
            ));
        }
        issues.extend(validate_object(object));
    }

    // B3: a join conveyor declares how many upstream belts it merges.
    for object in &model.objects {
        if let crate::model::ObjectParams::Conveyor(params) = &object.params
            && let Some(inputs) = params.join_inputs
        {
            let connected = model.inputs(&object.name).len() as u32;
            if inputs != connected {
                issues.push(ValidationIssue::new(
                    format!("objects.{}.join_inputs", object.name),
                    format!(
                        "join_inputs is {inputs} but the object has {connected} input connections"
                    ),
                ));
            }
        }
    }

    let mut output_ports: std::collections::BTreeMap<String, std::collections::BTreeSet<u32>> =
        std::collections::BTreeMap::new();
    for connection in &model.connections {
        issues.extend(validate_connection(model, connection, &mut output_ports));
    }

    for experiment in &model.experiments {
        for scenario in &experiment.scenarios {
            let location = format!("experiments.{}.{}", experiment.name, scenario.name);
            if let Some(stop) = scenario.stop_time
                && stop <= 0.0
            {
                issues.push(ValidationIssue::new(
                    format!("{location}.stop_time"),
                    "must be positive",
                ));
            }
            if let Some(warmup) = scenario.warmup
                && warmup < 0.0
            {
                issues.push(ValidationIssue::new(
                    format!("{location}.warmup"),
                    "cannot be negative",
                ));
            }
            let effective_stop = scenario.stop_time.unwrap_or(model.run.stop_time);
            let effective_warmup = scenario.warmup.unwrap_or(model.run.warmup);
            if effective_warmup >= effective_stop {
                issues.push(ValidationIssue::new(
                    format!("{location}.warmup"),
                    format!(
                        "warmup ({effective_warmup}) must be shorter than the effective stop_time ({effective_stop})"
                    ),
                ));
            }
            for over in &scenario.overrides {
                if model.object(&over.object).is_none() {
                    issues.push(ValidationIssue::new(
                        format!("experiments.{}.{}", experiment.name, scenario.name),
                        format!("override refers to unknown object '{}'", over.object),
                    ));
                }
                if over.param.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        format!("experiments.{}.{}", experiment.name, scenario.name),
                        "override needs a parameter name",
                    ));
                }
            }
        }
    }

    if !model
        .objects
        .iter()
        .any(|object| object.kind == ObjectKind::Source)
    {
        issues.push(ValidationIssue::new("objects", "the model has no source"));
    }
    if !model
        .objects
        .iter()
        .any(|object| object.kind == ObjectKind::Sink)
    {
        issues.push(ValidationIssue::new("objects", "the model has no sink"));
    }

    issues
}

fn validate_connection(
    model: &Model,
    connection: &Connection,
    output_ports: &mut std::collections::BTreeMap<String, std::collections::BTreeSet<u32>>,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let location = format!("connections {} -> {}", connection.from, connection.to);
    let Some(from) = model.object(&connection.from) else {
        issues.push(ValidationIssue::new(
            &location,
            "connection starts at an unknown object",
        ));
        return issues;
    };
    let Some(to) = model.object(&connection.to) else {
        issues.push(ValidationIssue::new(
            &location,
            "connection ends at an unknown object",
        ));
        return issues;
    };
    if connection.from == connection.to {
        issues.push(ValidationIssue::new(
            &location,
            "an object cannot connect to itself",
        ));
    }
    if from.kind == ObjectKind::Sink {
        issues.push(ValidationIssue::new(
            &location,
            "a sink has no output connections",
        ));
    }
    if to.kind == ObjectKind::Source {
        issues.push(ValidationIssue::new(
            &location,
            "a source has no input connections",
        ));
    }

    // Port numbers must be unique per source object, and explicit ports must
    // leave room for the implicit ones.
    let set = output_ports.entry(connection.from.clone()).or_default();
    let port = connection.port.unwrap_or(set.len() as u32);
    if !set.insert(port) {
        issues.push(ValidationIssue::new(
            &location,
            format!("duplicate output port {port} on '{}'", connection.from),
        ));
    }

    if let Some(strategy) = &connection.strategy {
        let outputs = model.outputs(&connection.from).len() as u32;
        match strategy {
            PortStrategy::ByPercentage { percentages } => {
                if percentages.len() != outputs as usize {
                    issues.push(ValidationIssue::new(
                        &location,
                        format!(
                            "by_percentage needs one percentage per output connection ({outputs}), found {}",
                            percentages.len()
                        ),
                    ));
                }
                let sum: f64 = percentages.iter().sum();
                if (sum - 100.0).abs() > 0.01 {
                    issues.push(ValidationIssue::new(
                        &location,
                        format!("percentages must sum to 100, found {sum}"),
                    ));
                }
                if percentages.iter().any(|p| !p.is_finite() || *p < 0.0) {
                    issues.push(ValidationIssue::new(
                        &location,
                        "percentages must be non-negative numbers",
                    ));
                }
            }
            PortStrategy::ByCondition { conditions } => {
                for condition in conditions {
                    if condition.port >= outputs {
                        issues.push(ValidationIssue::new(
                            &location,
                            format!(
                                "condition routes to port {}, but '{}' has {outputs} outputs",
                                condition.port, connection.from
                            ),
                        ));
                    }
                }
            }
            PortStrategy::ByItemType | PortStrategy::ByLabel { .. } => {
                if outputs == 0 {
                    issues.push(ValidationIssue::new(
                        &location,
                        "routing needs at least one output",
                    ));
                }
            }
            PortStrategy::FirstAvailable
            | PortStrategy::RoundRobin
            | PortStrategy::Random
            | PortStrategy::Hold => {}
        }
    }

    issues
}

fn validate_object(object: &Object) -> Vec<ValidationIssue> {
    let location = |field: &str| format!("objects.{}.{}", object.name, field);
    let mut issues = Vec::new();

    // B11: availability calendars are linear, ordered and non-overlapping.
    if let Some(timetable) = object.params.timetable() {
        let mut previous_end = f64::NEG_INFINITY;
        for (i, window) in timetable.windows.iter().enumerate() {
            if window.end <= window.start {
                issues.push(ValidationIssue::new(
                    location(&format!("timetable.windows[{i}]")),
                    "end must be after start",
                ));
            }
            if window.start < previous_end {
                issues.push(ValidationIssue::new(
                    location(&format!("timetable.windows[{i}]")),
                    "windows must be ordered and must not overlap",
                ));
            }
            previous_end = window.end;
        }
    }
    macro_rules! distribution {
        ($field:expr, $dist:expr) => {
            if let Err(err) = $dist.validate() {
                issues.push(ValidationIssue::new(location($field), err));
            }
        };
    }

    match &object.params {
        crate::model::ObjectParams::Source(params) => {
            let interarrival_set =
                params.interarrival != crate::distribution::Distribution::default();
            let style = params.effective_arrival_style();
            if params.arrival_schedule.is_some() && params.arrival_sequence.is_some() {
                issues.push(ValidationIssue::new(
                    location("arrival_sequence"),
                    "arrival_schedule and arrival_sequence are mutually exclusive",
                ));
            }
            if style != ArrivalStyle::InterArrivalTime && interarrival_set {
                issues.push(ValidationIssue::new(
                    location("interarrival"),
                    "interarrival cannot be combined with arrival_schedule or arrival_sequence",
                ));
            }
            match style {
                ArrivalStyle::InterArrivalTime => {
                    if params.arrival_schedule.is_some() || params.arrival_sequence.is_some() {
                        issues.push(ValidationIssue::new(
                            location("arrival_style"),
                            "arrival_style is inter_arrival_time but a table arrival mode is configured",
                        ));
                    }
                    distribution!("interarrival", &params.interarrival);
                }
                ArrivalStyle::ArrivalSchedule => {
                    if let Some(schedule) = &params.arrival_schedule {
                        if schedule.rows.is_empty() {
                            issues.push(ValidationIssue::new(
                                location("arrival_schedule.rows"),
                                "must contain at least one row",
                            ));
                        }
                        let mut previous = f64::NEG_INFINITY;
                        for (index, row) in schedule.rows.iter().enumerate() {
                            if !row.time.is_finite() || row.time < 0.0 {
                                issues.push(ValidationIssue::new(
                                    location(&format!("arrival_schedule.rows[{index}].time")),
                                    "must be a non-negative finite time",
                                ));
                            } else if row.time < previous {
                                issues.push(ValidationIssue::new(
                                    location(&format!("arrival_schedule.rows[{index}].time")),
                                    "arrival times must be non-decreasing",
                                ));
                            } else {
                                previous = row.time;
                            }
                        }
                        if schedule.repeat && schedule.rows.iter().all(|row| row.quantity == 0) {
                            issues.push(ValidationIssue::new(
                                location("arrival_schedule"),
                                "a repeating schedule needs at least one row with quantity > 0",
                            ));
                        }
                    } else {
                        issues.push(ValidationIssue::new(
                            location("arrival_schedule"),
                            "arrival_style is arrival_schedule but arrival_schedule is missing",
                        ));
                    }
                }
                ArrivalStyle::ArrivalSequence => {
                    if let Some(sequence) = &params.arrival_sequence {
                        if sequence.is_empty() {
                            issues.push(ValidationIssue::new(
                                location("arrival_sequence"),
                                "must contain at least one row",
                            ));
                        }
                    } else {
                        issues.push(ValidationIssue::new(
                            location("arrival_sequence"),
                            "arrival_style is arrival_sequence but arrival_sequence is missing",
                        ));
                    }
                }
            }
            if params.items_per_arrival == 0 {
                issues.push(ValidationIssue::new(
                    location("items_per_arrival"),
                    "must be at least 1",
                ));
            }
            if params.first_arrival < 0.0 {
                issues.push(ValidationIssue::new(
                    location("first_arrival"),
                    "cannot be negative",
                ));
            }
            if let Some(max) = params.max_arrivals
                && max == 0
            {
                issues.push(ValidationIssue::new(
                    location("max_arrivals"),
                    "must be at least 1",
                ));
            }
            if let Some(stop) = params.stop_time
                && stop <= 0.0
            {
                issues.push(ValidationIssue::new(
                    location("stop_time"),
                    "must be positive",
                ));
            }
        }
        crate::model::ObjectParams::Queue(params) => {
            if params.max_content == 0 {
                issues.push(ValidationIssue::new(
                    location("max_content"),
                    "must be at least 1",
                ));
            }
            if let Some(batch) = params.batch_size
                && batch == 0
            {
                issues.push(ValidationIssue::new(
                    location("batch_size"),
                    "must be at least 1",
                ));
            }
            if params.batch_max_wait < 0.0 {
                issues.push(ValidationIssue::new(
                    location("batch_max_wait"),
                    "cannot be negative",
                ));
            }
        }
        crate::model::ObjectParams::Processor(params) => {
            distribution!("setup_time", &params.setup_time);
            distribution!("process_time", &params.process_time);
            distribution!("changeover_time", &params.changeover_time);
            if params.max_content == 0 {
                issues.push(ValidationIssue::new(
                    location("max_content"),
                    "must be at least 1",
                ));
            }
            for (field, value) in [
                ("num_setup_operators", params.num_setup_operators),
                ("num_process_operators", params.num_process_operators),
            ] {
                if value == Some(0) {
                    issues.push(ValidationIssue::new(
                        location(field),
                        "must be at least 1 when given",
                    ));
                }
            }
            if params.use_same_num_operators
                && params.num_process_operators.is_some()
                && params.num_setup_operators.is_some()
                && params.num_process_operators != params.num_setup_operators
            {
                issues.push(ValidationIssue::new(
                    location("num_process_operators"),
                    "must match num_setup_operators when use_same_num_operators is true",
                ));
            }
            if let Some(failure) = &params.failure {
                distribution!("failure.mtbf", &failure.mtbf);
                distribution!("failure.mttr", &failure.mttr);
                if let Some(first) = failure.first_failure_time
                    && !first.is_finite()
                {
                    issues.push(ValidationIssue::new(
                        location("failure.first_failure_time"),
                        "must be a finite time (negative values mean ignore)",
                    ));
                }
                for state in &failure.count_time_in_states {
                    if !is_runtime_state(state) {
                        issues.push(ValidationIssue::new(
                            location("failure.count_time_in_states"),
                            format!("unknown runtime state '{state}'"),
                        ));
                    }
                }
            }
        }
        crate::model::ObjectParams::MultiProcessor(params) => {
            distribution!("setup_time", &params.setup_time);
            distribution!("process_time", &params.process_time);
            if params.slots == 0 {
                issues.push(ValidationIssue::new(
                    location("slots"),
                    "must be at least 1",
                ));
            }
        }
        crate::model::ObjectParams::Conveyor(params) => {
            if params.speed <= 0.0 {
                issues.push(ValidationIssue::new(location("speed"), "must be positive"));
            }
            if params.length <= 0.0 {
                issues.push(ValidationIssue::new(location("length"), "must be positive"));
            }
            if params.item_spacing <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("item_spacing"),
                    "must be positive",
                ));
            }
            if params.max_content == 0 {
                issues.push(ValidationIssue::new(
                    location("max_content"),
                    "must be at least 1",
                ));
            }
            for (i, eye) in params.photo_eyes.iter().enumerate() {
                if !eye.position.is_finite() || eye.position <= 0.0 || eye.position >= params.length
                {
                    issues.push(ValidationIssue::new(
                        location(&format!("photo_eyes[{i}].position")),
                        "must be inside the belt (0 < position < length)",
                    ));
                }
            }
            if let Some(inputs) = params.join_inputs
                && inputs < 2
            {
                issues.push(ValidationIssue::new(
                    location("join_inputs"),
                    "a join conveyor needs at least two inputs",
                ));
            }
            for (i, station) in params.stations.iter().enumerate() {
                if !station.position.is_finite()
                    || station.position <= 0.0
                    || station.position >= params.length
                {
                    issues.push(ValidationIssue::new(
                        location(&format!("stations[{i}].position")),
                        "must be inside the belt (0 < position < length)",
                    ));
                }
                distribution!(
                    &format!("stations[{i}].process_time"),
                    &station.process_time
                );
            }
        }
        crate::model::ObjectParams::Combiner(params) => {
            distribution!("setup_time", &params.setup_time);
            distribution!("process_time", &params.process_time);
            if params.components_per_container == 0 {
                issues.push(ValidationIssue::new(
                    location("components_per_container"),
                    "must be at least 1",
                ));
            }
            if params.mode == CombinerMode::Batch && params.components_per_container == 0 {
                issues.push(ValidationIssue::new(
                    location("components_per_container"),
                    "batch mode needs at least one component per batch",
                ));
            }
        }
        crate::model::ObjectParams::Transporter(params) => {
            if params.capacity == 0 {
                issues.push(ValidationIssue::new(
                    location("capacity"),
                    "must be at least 1",
                ));
            }
            if params.speed <= 0.0 {
                issues.push(ValidationIssue::new(location("speed"), "must be positive"));
            }
            distribution!("load_time", &params.load_time);
            distribution!("unload_time", &params.unload_time);
            for (i, edge) in params.network.iter().enumerate() {
                if edge.from.trim().is_empty() || edge.to.trim().is_empty() {
                    issues.push(ValidationIssue::new(
                        location(&format!("network[{i}]")),
                        "network edges need a from and a to node",
                    ));
                }
                if !edge.distance.is_finite() || edge.distance <= 0.0 {
                    issues.push(ValidationIssue::new(
                        location(&format!("network[{i}].distance")),
                        "must be positive",
                    ));
                }
            }
        }
        crate::model::ObjectParams::Resource(params) => {
            if params.capacity == 0 {
                issues.push(ValidationIssue::new(
                    location("capacity"),
                    "must be at least 1",
                ));
            }
            for (i, window) in params.downtimes.iter().enumerate() {
                if window.end <= window.start {
                    issues.push(ValidationIssue::new(
                        location(&format!("downtimes[{i}]")),
                        "end must be after start",
                    ));
                }
            }
        }
        crate::model::ObjectParams::Rack(params) => {
            if let Some(channels) = params.channels
                && channels == 0
            {
                issues.push(ValidationIssue::new(
                    location("channels"),
                    "must be at least 1",
                ));
            }
            if let Some(depth) = params.depth
                && depth == 0
            {
                issues.push(ValidationIssue::new(
                    location("depth"),
                    "must be at least 1",
                ));
            }
            if params.capacity == 0 {
                issues.push(ValidationIssue::new(
                    location("capacity"),
                    "must be at least 1",
                ));
            }
            if params.levels == 0 {
                issues.push(ValidationIssue::new(
                    location("levels"),
                    "must be at least 1",
                ));
            }
        }
        crate::model::ObjectParams::BasicTe(params) => {
            if params.speed <= 0.0 {
                issues.push(ValidationIssue::new(location("speed"), "must be positive"));
            }
            if params.capacity == 0 {
                issues.push(ValidationIssue::new(
                    location("capacity"),
                    "must be at least 1",
                ));
            }
        }
        crate::model::ObjectParams::Separator(params) => {
            if params.separate_mode == SeparateMode::Split && params.split_count == 0 {
                issues.push(ValidationIssue::new(
                    location("split_count"),
                    "split mode needs at least one item (split_count >= 1)",
                ));
            }
        }
        crate::model::ObjectParams::Sink(_)
        | crate::model::ObjectParams::Merge(_)
        | crate::model::ObjectParams::Split(_)
        | crate::model::ObjectParams::Dispatcher(_)
        | crate::model::ObjectParams::BasicFr(_) => {}
    }

    issues
}

/// Runtime states a `failure.count_time_in_states` entry may name (A8).
fn is_runtime_state(name: &str) -> bool {
    matches!(
        name,
        "idle"
            | "setup"
            | "processing"
            | "blocked"
            | "waiting_for_operator"
            | "waiting_for_transport"
            | "down"
            | "off_shift"
    )
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::distribution::Distribution;
    use crate::model::*;

    fn base_model() -> Model {
        Model {
            format: FORMAT_NAME.into(),
            version: FORMAT_VERSION,
            name: "test".into(),
            description: String::new(),
            time_unit: TimeUnit::Seconds,
            run: RunSettings::default(),
            objects: vec![
                Object::new(
                    "Source1",
                    ObjectParams::Source(SourceParams {
                        interarrival: Distribution::Exponential { mean: 5.0 },
                        ..Default::default()
                    }),
                ),
                Object::new("Sink1", ObjectParams::Sink(SinkParams::default())),
            ],
            connections: vec![Connection {
                from: "Source1".into(),
                to: "Sink1".into(),
                port: None,
                strategy: None,
            }],
            experiments: Vec::new(),
            metadata: Default::default(),
        }
    }

    #[test]
    fn accepts_a_simple_model() {
        let result = validate(&base_model());
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn rejects_duplicate_names() {
        let mut model = base_model();
        model.objects.push(Object::new(
            "Source1",
            ObjectParams::Sink(SinkParams::default()),
        ));
        let issues = validate_collect(&model);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("duplicate"))
        );
    }

    #[test]
    fn rejects_unknown_connection_target() {
        let mut model = base_model();
        model.connections.push(Connection {
            from: "Source1".into(),
            to: "Ghost".into(),
            port: None,
            strategy: None,
        });
        let issues = validate_collect(&model);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("unknown object"))
        );
    }

    #[test]
    fn rejects_bad_percentages() {
        let mut model = base_model();
        model.objects.push(Object::new(
            "Sink2",
            ObjectParams::Sink(SinkParams::default()),
        ));
        model.connections.clear();
        model.connections.push(Connection {
            from: "Source1".into(),
            to: "Sink1".into(),
            port: None,
            strategy: Some(PortStrategy::ByPercentage {
                percentages: vec![50.0, 40.0],
            }),
        });
        model.connections.push(Connection {
            from: "Source1".into(),
            to: "Sink2".into(),
            port: None,
            strategy: None,
        });
        let issues = validate_collect(&model);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("sum to 100"))
        );
    }
}
