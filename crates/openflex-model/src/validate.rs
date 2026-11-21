//! Model validation.
//!
//! Every problem that can be detected without running the simulation is
//! reported here, with the location of the offending object. `openflex
//! validate` prints this list, so a model never fails halfway through a run
//! because of a typo.

use crate::model::{
    ArrivalStyle, CombinerMode, Connection, ConnectionKind, Model, ModelError, Object, ObjectKind,
    PortStrategy, SeparateMode, ValidationIssue,
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
    if model.objects.is_empty() && model.process_flow.is_none() {
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
        if !object.z.is_finite() {
            issues.push(ValidationIssue::new(
                format!("objects.{}.z", object.name),
                "height must be finite",
            ));
        }
        if let Some(rot) = object.rot
            && !rot.is_finite()
        {
            issues.push(ValidationIssue::new(
                format!("objects.{}.rot", object.name),
                "rotation must be a finite number of degrees",
            ));
        }
        issues.extend(validate_object(model, object));
        if let crate::model::ObjectParams::Agv(params) = &object.params
            && let Some(charger) = &params.charger
            && model.object(charger).is_none()
        {
            issues.push(ValidationIssue::new(
                format!("objects.{}.charger", object.name),
                format!("unknown charger object '{charger}'"),
            ));
        }
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

    issues.extend(validate_global_tables(model));
    issues.extend(validate_global_variables(model));
    issues.extend(validate_picklists(model));

    let mut output_ports: std::collections::BTreeMap<String, std::collections::BTreeSet<u32>> =
        std::collections::BTreeMap::new();
    let object_index: std::collections::HashMap<&str, usize> = model
        .objects
        .iter()
        .enumerate()
        .map(|(index, object)| (object.name.as_str(), index))
        .collect();
    for connection in &model.connections {
        issues.extend(validate_connection(
            model,
            &object_index,
            connection,
            &mut output_ports,
        ));
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

    // A process-flow-only or fluid-only model has no source/sink objects; mixed
    // models may still rely on those subsystems, so the object requirements
    // only apply when neither is declared.
    let has_fluid = model
        .objects
        .iter()
        .any(|object| is_fluid_kind(object.kind));
    if model.process_flow.is_none() && !has_fluid {
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
    }

    issues.extend(validate_process_flow(
        model,
        model.process_flow.as_ref(),
        None,
    ));
    for object in &model.objects {
        if let Some(flow) = object.params.process_flow() {
            issues.extend(validate_process_flow(model, Some(flow), Some(object)));
        }
    }
    issues.extend(validate_fluid(model));
    issues.extend(validate_model_triggers(model));
    issues.extend(validate_performance_measures(model));
    issues.extend(validate_tracked_variables(model));

    issues
}

fn validate_connection(
    model: &Model,
    object_index: &std::collections::HashMap<&str, usize>,
    connection: &Connection,
    output_ports: &mut std::collections::BTreeMap<String, std::collections::BTreeSet<u32>>,
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let location = format!("connections {} -> {}", connection.from, connection.to);
    let Some(&from_index) = object_index.get(connection.from.as_str()) else {
        issues.push(ValidationIssue::new(
            &location,
            "connection starts at an unknown object",
        ));
        return issues;
    };
    let Some(&to_index) = object_index.get(connection.to.as_str()) else {
        issues.push(ValidationIssue::new(
            &location,
            "connection ends at an unknown object",
        ));
        return issues;
    };
    let from = &model.objects[from_index];
    let to = &model.objects[to_index];
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

    // S connections are center-port/resource links: they do not route flow
    // items, so they carry no port number or port strategy, and they never
    // connect a source or a sink (those have no center-port semantics).
    if connection.kind == Some(ConnectionKind::S) {
        if connection.port.is_some() {
            issues.push(ValidationIssue::new(
                &location,
                "an S connection cannot specify an output port",
            ));
        }
        if connection.strategy.is_some() {
            issues.push(ValidationIssue::new(
                &location,
                "an S connection cannot use a port strategy",
            ));
        }
        if !is_center_port_object(from.kind) || !is_center_port_object(to.kind) {
            issues.push(ValidationIssue::new(
                &location,
                "an S connection links fixed resources or task executers; sources and sinks cannot be endpoints",
            ));
        }
        // S connections must not consume an A output port number.
        return issues;
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
                validate_conditional_ports(conditions, &location, &mut issues);
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

fn validate_object(model: &Model, object: &Object) -> Vec<ValidationIssue> {
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
            if params.arrival_schedule_from_table.is_some()
                && (params.arrival_schedule.is_some() || params.arrival_sequence.is_some())
            {
                issues.push(ValidationIssue::new(
                    location("arrival_schedule_from_table"),
                    "arrival_schedule_from_table cannot be combined with arrival_schedule or arrival_sequence",
                ));
            }
            if style != ArrivalStyle::InterArrivalTime && interarrival_set {
                issues.push(ValidationIssue::new(
                    location("interarrival"),
                    "interarrival cannot be combined with a table arrival mode",
                ));
            }
            if let Some(table_name) = &params.arrival_schedule_from_table {
                if let Some(explicit) = params.arrival_style
                    && explicit != ArrivalStyle::ArrivalSchedule
                {
                    issues.push(ValidationIssue::new(
                        location("arrival_style"),
                        "arrival_style must be arrival_schedule when arrival_schedule_from_table is set",
                    ));
                }
                match model.apply_table_reference(table_name) {
                    Err(message) => issues.push(ValidationIssue::new(
                        location("arrival_schedule_from_table"),
                        message,
                    )),
                    Ok(schedule) => {
                        let mut previous = f64::NEG_INFINITY;
                        for (index, row) in schedule.rows.iter().enumerate() {
                            if row.time < previous {
                                issues.push(ValidationIssue::new(
                                    location(&format!(
                                        "arrival_schedule_from_table.rows[{index}].time"
                                    )),
                                    "arrival times must be non-decreasing",
                                ));
                            } else {
                                previous = row.time;
                            }
                        }
                    }
                }
            }
            match style {
                ArrivalStyle::InterArrivalTime => {
                    if params.arrival_schedule.is_some()
                        || params.arrival_sequence.is_some()
                        || params.arrival_schedule_from_table.is_some()
                    {
                        issues.push(ValidationIssue::new(
                            location("arrival_style"),
                            "arrival_style is inter_arrival_time but a table arrival mode is configured",
                        ));
                    }
                    distribution!("interarrival", &params.interarrival);
                }
                ArrivalStyle::ArrivalSchedule => {
                    if params.arrival_schedule_from_table.is_some() {
                        // Resolved from a global table above.
                    } else if let Some(schedule) = &params.arrival_schedule {
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
            if let Some(curve) = params.curve {
                if !curve.radius.is_finite() || curve.radius <= 0.0 {
                    issues.push(ValidationIssue::new(
                        location("curve.radius"),
                        "must be positive",
                    ));
                }
                if !curve.angle.is_finite() || curve.angle <= 0.0 || curve.angle > 360.0 {
                    issues.push(ValidationIssue::new(
                        location("curve.angle"),
                        "must be between 0 and 360 degrees",
                    ));
                }
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
            if let Some(gap) = params.slug_min_gap
                && (!gap.is_finite() || gap <= 0.0)
            {
                issues.push(ValidationIssue::new(
                    location("slug_min_gap"),
                    "must be positive",
                ));
            }
            match params.merge_mode {
                crate::model::MergeMode::RoundRobin | crate::model::MergeMode::ByItemType => {
                    if params.merge_weight.is_some() {
                        issues.push(ValidationIssue::new(
                            location("merge_weight"),
                            "merge_weight is only valid with merge_mode: weighted",
                        ));
                    }
                }
                crate::model::MergeMode::Weighted => {
                    let expected = params.join_inputs.unwrap_or(0) as usize;
                    match params.merge_weight.as_deref() {
                        None => issues.push(ValidationIssue::new(
                            location("merge_weight"),
                            "weighted merge mode requires merge_weight",
                        )),
                        Some(weights) if weights.len() != expected => {
                            issues.push(ValidationIssue::new(
                                location("merge_weight"),
                                format!(
                                    "expected {expected} weights (one per input), found {}",
                                    weights.len()
                                ),
                            ))
                        }
                        Some(weights)
                            if weights
                                .iter()
                                .any(|weight| !weight.is_finite() || *weight <= 0.0) =>
                        {
                            issues.push(ValidationIssue::new(
                                location("merge_weight"),
                                "every weight must be positive and finite",
                            ));
                        }
                        Some(_) => {}
                    }
                }
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
                if let Some(range) = eye.range
                    && (!range.is_finite() || range <= 0.0)
                {
                    issues.push(ValidationIssue::new(
                        location(&format!("photo_eyes[{i}].range")),
                        "must be positive",
                    ));
                }
                for (trigger, actions) in [
                    ("on_blocked", &eye.triggers.on_blocked),
                    ("on_cleared", &eye.triggers.on_cleared),
                ] {
                    for (position, action) in actions.iter().enumerate() {
                        if let crate::action::Action::Script { source } = action {
                            validate_script_source(
                                source,
                                &format!(
                                    "objects.{}.photo_eyes[{i}].triggers.{trigger}[{position}].source",
                                    object.name
                                ),
                                &mut issues,
                            );
                        }
                    }
                }
            }
            for (i, point) in params.decision_points.iter().enumerate() {
                if !point.position.is_finite()
                    || point.position <= 0.0
                    || point.position >= params.length
                {
                    issues.push(ValidationIssue::new(
                        location(&format!("decision_points[{i}].position")),
                        "must be inside the belt (0 < position < length)",
                    ));
                }
                if !point.range.is_finite() || point.range <= 0.0 {
                    issues.push(ValidationIssue::new(
                        location(&format!("decision_points[{i}].range")),
                        "must be positive",
                    ));
                }
                for (position, action) in point.actions.iter().enumerate() {
                    if let crate::action::Action::Script { source } = action {
                        validate_script_source(
                            source,
                            &format!(
                                "objects.{}.decision_points[{i}].actions[{position}].source",
                                object.name
                            ),
                            &mut issues,
                        );
                    }
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
        crate::model::ObjectParams::Escalator(params) => {
            if params.speed <= 0.0 {
                issues.push(ValidationIssue::new(location("speed"), "must be positive"));
            }
            if let Some(curve) = params.curve {
                if !curve.radius.is_finite() || curve.radius <= 0.0 {
                    issues.push(ValidationIssue::new(
                        location("curve.radius"),
                        "must be positive",
                    ));
                }
                if !curve.angle.is_finite() || curve.angle <= 0.0 || curve.angle > 360.0 {
                    issues.push(ValidationIssue::new(
                        location("curve.angle"),
                        "must be between 0 and 360 degrees",
                    ));
                }
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
        crate::model::ObjectParams::Robot(params) => {
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
        crate::model::ObjectParams::Crane(params) => {
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
        crate::model::ObjectParams::AsrsVehicle(params) => {
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
        crate::model::ObjectParams::Person(params) => {
            if params.capacity != 1 {
                issues.push(ValidationIssue::new(
                    location("capacity"),
                    "a person always has capacity 1",
                ));
            }
            if params.speed <= 0.0 {
                issues.push(ValidationIssue::new(location("speed"), "must be positive"));
            }
            distribution!("load_time", &params.load_time);
            distribution!("unload_time", &params.unload_time);
        }
        crate::model::ObjectParams::Agv(params) => {
            if let Some(capacity) = params.battery_capacity
                && (!capacity.is_finite() || capacity <= 0.0)
            {
                issues.push(ValidationIssue::new(
                    location("battery_capacity"),
                    "must be positive when present",
                ));
            }
            if let Some(consumption) = params.consumption_per_distance
                && (!consumption.is_finite() || consumption <= 0.0)
            {
                issues.push(ValidationIssue::new(
                    location("consumption_per_distance"),
                    "must be positive when present",
                ));
            }
            distribution!("charge_time", &params.charge_time);
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
        crate::model::ObjectParams::Elevator(params) => {
            if params.capacity == 0 {
                issues.push(ValidationIssue::new(
                    location("capacity"),
                    "must be at least 1",
                ));
            }
            if params.speed <= 0.0 || !params.speed.is_finite() {
                issues.push(ValidationIssue::new(location("speed"), "must be positive"));
            }
            distribution!("stop_time", &params.stop_time);
        }
        crate::model::ObjectParams::FluidGenerator(params) => {
            if !params.flow_rate.is_finite() || params.flow_rate <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("flow_rate"),
                    "must be positive",
                ));
            }
            if let Some(stop) = params.stop_time
                && (!stop.is_finite() || stop <= 0.0)
            {
                issues.push(ValidationIssue::new(
                    location("stop_time"),
                    "must be positive when present",
                ));
            }
        }
        crate::model::ObjectParams::FluidTank(params) => {
            if !params.capacity.is_finite() || params.capacity <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("capacity"),
                    "must be positive",
                ));
            }
            if !params.initial_level.is_finite()
                || params.initial_level < 0.0
                || params.initial_level > params.capacity
            {
                issues.push(ValidationIssue::new(
                    location("initial_level"),
                    "must be between 0 and capacity",
                ));
            }
            if let Some(outflow) = params.max_outflow
                && (!outflow.is_finite() || outflow < 0.0)
            {
                issues.push(ValidationIssue::new(
                    location("max_outflow"),
                    "must be non-negative when present",
                ));
            }
            for (field, value) in [("density", params.density), ("gravity", params.gravity)] {
                if let Some(value) = value
                    && (!value.is_finite() || value <= 0.0)
                {
                    issues.push(ValidationIssue::new(
                        location(field),
                        "must be positive when present",
                    ));
                }
            }
        }
        crate::model::ObjectParams::FluidValve(params) => {
            if !params.ratio.is_finite() || !(0.0..=1.0).contains(&params.ratio) {
                issues.push(ValidationIssue::new(
                    location("ratio"),
                    "must be between 0 and 1",
                ));
            }
            for (field, value) in [
                ("pressure_threshold", params.pressure_threshold),
                ("pressure_open", params.pressure_open),
                ("pressure_close", params.pressure_close),
            ] {
                if let Some(value) = value
                    && (!value.is_finite() || value < 0.0)
                {
                    issues.push(ValidationIssue::new(
                        location(field),
                        "must be non-negative when present",
                    ));
                }
            }
            if let (Some(open), Some(close)) = (params.pressure_open, params.pressure_close)
                && open < close
            {
                issues.push(ValidationIssue::new(
                    location("pressure_open"),
                    "must not be below pressure_close",
                ));
            }
        }
        crate::model::ObjectParams::FluidPipe(params) => {
            if !params.length.is_finite() || params.length <= 0.0 {
                issues.push(ValidationIssue::new(location("length"), "must be positive"));
            }
            if !params.speed.is_finite() || params.speed <= 0.0 {
                issues.push(ValidationIssue::new(location("speed"), "must be positive"));
            }
            if let Some(delay) = params.delay
                && (!delay.is_finite() || delay < 0.0)
            {
                issues.push(ValidationIssue::new(
                    location("delay"),
                    "must be non-negative when present",
                ));
            }
            if let Some(rate) = params.flow_rate
                && (!rate.is_finite() || rate <= 0.0)
            {
                issues.push(ValidationIssue::new(
                    location("flow_rate"),
                    "must be positive when present",
                ));
            }
            if let Some(temperature) = params.temperature
                && !temperature.is_finite()
            {
                issues.push(ValidationIssue::new(
                    location("temperature"),
                    "must be finite when present",
                ));
            }
        }
        crate::model::ObjectParams::Ticker(params) => {
            if !params.tick_length.is_finite() || params.tick_length <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("tick_length"),
                    "must be positive",
                ));
            }
        }
        crate::model::ObjectParams::FluidMixer(params) => {
            if !params.product_id.is_finite() || params.product_id < 0.0 {
                issues.push(ValidationIssue::new(
                    location("product_id"),
                    "must be non-negative",
                ));
            }
            for (index, step) in params.steps.iter().enumerate() {
                if step
                    .pull
                    .iter()
                    .any(|pull| !pull.is_finite() || *pull < 0.0)
                {
                    issues.push(ValidationIssue::new(
                        location(&format!("steps[{index}].pull")),
                        "pull volumes must be finite and non-negative",
                    ));
                }
                if !step.delay.is_finite() || step.delay < 0.0 {
                    issues.push(ValidationIssue::new(
                        location(&format!("steps[{index}].delay")),
                        "must be non-negative",
                    ));
                }
            }
        }
        crate::model::ObjectParams::FluidBlender(params) => {
            if !params.output_rate.is_finite() || params.output_rate <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("output_rate"),
                    "must be positive",
                ));
            }
            if params
                .ratios
                .iter()
                .any(|ratio| !ratio.is_finite() || *ratio < 0.0)
            {
                issues.push(ValidationIssue::new(
                    location("ratios"),
                    "ratios must be finite and non-negative",
                ));
            }
            if params.ratios.iter().sum::<f64>() <= 0.0 && !params.ratios.is_empty() {
                issues.push(ValidationIssue::new(
                    location("ratios"),
                    "at least one ratio must be greater than zero",
                ));
            }
            if !params.loss.is_finite() || !(0.0..1.0).contains(&params.loss) {
                issues.push(ValidationIssue::new(
                    location("loss"),
                    "must be between 0 and 1",
                ));
            }
        }
        crate::model::ObjectParams::FluidSplitter(params) => {
            if !params.output_rate.is_finite() || params.output_rate < 0.0 {
                issues.push(ValidationIssue::new(
                    location("output_rate"),
                    "must be non-negative",
                ));
            }
            for (index, output) in params.outputs.iter().enumerate() {
                if !output.percent.is_finite() || !(0.0..=100.0).contains(&output.percent) {
                    issues.push(ValidationIssue::new(
                        location(&format!("outputs[{index}].percent")),
                        "must be between 0 and 100",
                    ));
                }
            }
            if params
                .outputs
                .iter()
                .map(|output| output.percent)
                .sum::<f64>()
                > 100.0 + 1e-9
                && !params.outputs.is_empty()
            {
                issues.push(ValidationIssue::new(
                    location("outputs"),
                    "the split percentages must not add up to more than 100",
                ));
            }
        }
        crate::model::ObjectParams::FluidProcessor(params) => {
            if !params.max_output_rate.is_finite() || params.max_output_rate <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("max_output_rate"),
                    "must be positive",
                ));
            }
            if !params.max_capacity.is_finite() || params.max_capacity <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("max_capacity"),
                    "must be positive",
                ));
            }
            if !params.loss.is_finite() || !(0.0..1.0).contains(&params.loss) {
                issues.push(ValidationIssue::new(
                    location("loss"),
                    "must be between 0 and 1",
                ));
            }
        }
        crate::model::ObjectParams::ItemToFluid(params) => {
            if !params.fluid_per_unit.is_finite() || params.fluid_per_unit <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("fluid_per_unit"),
                    "must be positive",
                ));
            }
            if !params.units_per_item.is_finite() || params.units_per_item <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("units_per_item"),
                    "must be positive",
                ));
            }
        }
        crate::model::ObjectParams::FluidToItem(params) => {
            if !params.fluid_per_item.is_finite() || params.fluid_per_item <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("fluid_per_item"),
                    "must be positive",
                ));
            }
            if !params.units_per_item.is_finite() || params.units_per_item <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("units_per_item"),
                    "must be positive",
                ));
            }
        }
        crate::model::ObjectParams::NetworkNode(params) => {
            for (index, edge) in params.edges.iter().enumerate() {
                if !edge.distance.is_finite() || edge.distance < 0.0 {
                    issues.push(ValidationIssue::new(
                        location(&format!("edges[{index}].distance")),
                        "must be finite and non-negative",
                    ));
                }
            }
        }
        crate::model::ObjectParams::TrafficControl(params) => {
            if params.capacity == 0 {
                issues.push(ValidationIssue::new(
                    location("capacity"),
                    "must be at least 1 (0 would forbid every traveller)",
                ));
            }
            if !params.speed_factor.is_finite() || params.speed_factor <= 0.0 {
                issues.push(ValidationIssue::new(
                    location("speed_factor"),
                    "must be positive",
                ));
            }
        }
        crate::model::ObjectParams::VisualTool(params) => {
            if params
                .size
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
            {
                issues.push(ValidationIssue::new(
                    location("size"),
                    "every dimension must be positive",
                ));
            }
            if let Some(color) = params.color.as_deref()
                && !is_hex_color(color)
            {
                issues.push(ValidationIssue::new(
                    location("color"),
                    "must be a hexadecimal colour such as #336699",
                ));
            }
        }
        crate::model::ObjectParams::FluidSink(_) => {}
        crate::model::ObjectParams::Dispatcher(params) => {
            if params.task_sequence_mode == crate::model::TaskSequenceMode::Explicit {
                if params.task_sequence.is_empty() {
                    issues.push(ValidationIssue::new(
                        location("task_sequence"),
                        "explicit mode requires a non-empty task route",
                    ));
                }
                if !params.task_sequence.iter().any(|step| {
                    step.kind() == crate::model::TaskStepKind::Travel && step.extra_kind().is_none()
                }) {
                    issues.push(ValidationIssue::new(
                        location("task_sequence"),
                        "explicit mode requires at least one travel leg",
                    ));
                }
                for (position, step) in params.task_sequence.iter().enumerate() {
                    let at = format!("task_sequence[{position}]");
                    if step.extra_kind().is_some() {
                        // utilize/break: duration checked by the generic block below.
                        if step.target().is_some() {
                            issues.push(ValidationIssue::new(
                                location(&format!("{at}.target")),
                                "only travel legs may have a target",
                            ));
                        }
                        continue;
                    }
                    match step.kind() {
                        crate::model::TaskStepKind::Travel => {
                            let Some(target) = step.target() else {
                                issues.push(ValidationIssue::new(
                                    location(&format!("{at}.target")),
                                    "explicit travel legs require a target",
                                ));
                                continue;
                            };
                            match target {
                                crate::model::TravelTarget::Named(
                                    crate::model::TravelTargetName::Home
                                    | crate::model::TravelTargetName::Pickup
                                    | crate::model::TravelTargetName::Requester,
                                ) => {}
                                crate::model::TravelTarget::Object(object) => {
                                    if !model
                                        .objects
                                        .iter()
                                        .any(|entry| entry.name == object.object)
                                    {
                                        issues.push(ValidationIssue::new(
                                            location(&format!("{at}.target.object")),
                                            format!("unknown object \"{}\"", object.object),
                                        ));
                                    }
                                }
                                crate::model::TravelTarget::Position(position) => {
                                    if position.position.iter().any(|value| !value.is_finite()) {
                                        issues.push(ValidationIssue::new(
                                            location(&format!("{at}.target.position")),
                                            "coordinates must be finite",
                                        ));
                                    }
                                }
                            }
                        }
                        crate::model::TaskStepKind::Load | crate::model::TaskStepKind::Unload => {
                            if step.target().is_some() {
                                issues.push(ValidationIssue::new(
                                    location(&format!("{at}.target")),
                                    "only travel legs may have a target",
                                ));
                            }
                        }
                    }
                }
            } else if params
                .task_sequence
                .iter()
                .any(|step| step.target().is_some())
            {
                issues.push(ValidationIssue::new(
                    location("task_sequence"),
                    "target is only valid in explicit task_sequence_mode",
                ));
            }
            for (position, step) in params.task_sequence.iter().enumerate() {
                let field = format!("task_sequence[{position}].duration");
                if let Some(_extra) = step.extra_kind() {
                    let Some(duration) = step.duration() else {
                        issues.push(ValidationIssue::new(
                            location(&field),
                            "utilize/break legs require an explicit duration",
                        ));
                        continue;
                    };
                    if let Err(error) = duration.validate() {
                        issues.push(ValidationIssue::new(location(&field), error));
                    } else if duration.mean().is_none_or(|mean| mean <= 0.0) {
                        issues.push(ValidationIssue::new(
                            location(&field),
                            "utilize/break duration must be positive",
                        ));
                    }
                } else if let Some(duration) = step.duration()
                    && let Err(error) = duration.validate()
                {
                    issues.push(ValidationIssue::new(location(&field), error));
                }
            }
        }
        crate::model::ObjectParams::Sink(_)
        | crate::model::ObjectParams::Merge(_)
        | crate::model::ObjectParams::Split(_)
        | crate::model::ObjectParams::BasicFr(_) => {}
    }

    issues.extend(validate_trigger_scripts(object));
    issues
}

/// Performance-measure names are identifiers and unique; expressions must be
/// parseable **and** their model symbols must resolve, so a run cannot fail with
/// a late syntax or "unknown global variable" error (task-157).
fn validate_performance_measures(model: &Model) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for (position, measure) in model.performance_measures.iter().enumerate() {
        let at = format!("performance_measures[{position}]");
        if !is_identifier(&measure.name) {
            issues.push(ValidationIssue::new(
                format!("{at}.name"),
                "must be a valid identifier (letters, digits and _; not starting with a digit)",
            ));
        } else if !names.insert(measure.name.as_str()) {
            issues.push(ValidationIssue::new(
                format!("{at}.name"),
                "duplicate performance measure name",
            ));
        }
        if measure.expression.trim().is_empty() {
            issues.push(ValidationIssue::new(
                format!("{at}.expression"),
                "expression cannot be empty",
            ));
        } else {
            match crate::expr::Expr::parse(&measure.expression) {
                Err(error) => issues.push(ValidationIssue::new(
                    format!("{at}.expression"),
                    format!("expression does not compile: {error}"),
                )),
                // Only *symbol* problems are checked here. Divide-by-zero, type
                // errors and unknown engine statistics are run-time concerns and
                // must not turn `openflex validate` red (task-157).
                Ok(expression) => {
                    let location = format!("{at}.expression");
                    check_expression_symbols(model, &expression, &location, &mut issues);
                }
            }
        }
    }
    issues
}

/// Tracked variables (FlexSim Tracked Variables): identifier names that are
/// unique, a compileable expression whose model symbols resolve, and a positive
/// sampling interval (task-160).
///
/// This duplicates nothing: the checks live here so that every caller of
/// [`validate_collect`] sees them, not only `Model::validate`.
fn validate_tracked_variables(model: &Model) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let mut names = std::collections::BTreeSet::new();
    for (position, variable) in model.tracked_variables.iter().enumerate() {
        let at = format!("tracked_variables[{position}]");
        if !is_identifier(&variable.name) {
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
        } else {
            match crate::expr::Expr::parse(&variable.expression) {
                Err(error) => issues.push(ValidationIssue::new(
                    format!("{at}.expression"),
                    format!("expression does not compile: {error}"),
                )),
                // Same split as performance measures: symbols are checked here,
                // run-time arithmetic errors are left to the run (task-157).
                Ok(expression) => {
                    let location = format!("{at}.expression");
                    check_expression_symbols(model, &expression, &location, &mut issues);
                }
            }
        }
    }
    if let Some(interval) = model.tracked_interval
        && (!interval.is_finite() || interval <= 0.0)
    {
        issues.push(ValidationIssue::new(
            "tracked_interval",
            "must be a positive number of time units",
        ));
    }
    issues
}

/// Check that every model symbol an expression names exists.
///
/// This walks the expression tree rather than evaluating it, so a symbol inside
/// a short-circuited branch (`false && $Missing`) is still reported. Only the
/// two model-owned spellings are checked:
///
/// * `$Name` — must be declared under `global_variables` (a bare `Name` may be
///   an engine statistic such as `completed`, so it is left alone);
/// * `picklist("<list>", "<entry>")` with literal arguments — the list and the
///   entry must exist.
///
/// Messages come from [`ExprError`](crate::expr::ExprError)'s `Display`, so the
/// wording stays identical to the run-time error the engine would raise.
fn check_expression_symbols(
    model: &Model,
    expression: &crate::expr::Expr,
    location: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    use crate::expr::{Expr, ExprError};

    match expression {
        Expr::Path(path) => {
            if let Some(name) = path.strip_prefix('$')
                && model.global_variable(name).is_none()
            {
                issues.push(ValidationIssue::new(
                    location,
                    ExprError::UnknownGlobalVariable(name.to_string()).to_string(),
                ));
            }
        }
        Expr::Call(name, args) => {
            if name == "picklist"
                && let [Expr::Str(list), Expr::Str(entry)] = args.as_slice()
            {
                match model.picklist(list) {
                    None => issues.push(ValidationIssue::new(
                        location,
                        ExprError::UnknownPickList(list.clone()).to_string(),
                    )),
                    Some(picklist) if picklist.value_of(entry).is_none() => {
                        issues.push(ValidationIssue::new(
                            location,
                            ExprError::UnknownPickListEntry {
                                list: list.clone(),
                                entry: entry.clone(),
                            }
                            .to_string(),
                        ));
                    }
                    Some(_) => {}
                }
            }
            // `query("<sql>")`: resolve the table (and the columns the
            // statement names) at validate time with the very same parser the
            // engine uses, so the error text is identical. Only a string
            // literal can be checked; a computed statement is left to the run.
            if name == "query"
                && let [Expr::Str(sql)] = args.as_slice()
            {
                let context = crate::expr::ModelEvalContext::new(model);
                if let Err(error) = crate::sql::check(&context, sql) {
                    issues.push(ValidationIssue::new(location, error.to_string()));
                }
            }
            for argument in args {
                check_expression_symbols(model, argument, location, issues);
            }
        }
        Expr::Unary(_, inner) => check_expression_symbols(model, inner, location, issues),
        Expr::Binary(_, left, right) => {
            check_expression_symbols(model, left, location, issues);
            check_expression_symbols(model, right, location, issues);
        }
        Expr::Number(_) | Expr::Bool(_) | Expr::Str(_) => {}
    }
}

/// Model-level trigger actions (scripts must compile before the run starts).
fn validate_model_triggers(model: &Model) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    let triggers = &model.model_triggers;
    let lists = [
        ("on_start", &triggers.on_start),
        ("on_reset", &triggers.on_reset),
        ("on_stop", &triggers.on_stop),
        ("start_of_experiment", &triggers.start_of_experiment),
        ("end_of_experiment", &triggers.end_of_experiment),
        ("start_of_scenario", &triggers.start_of_scenario),
        ("end_of_scenario", &triggers.end_of_scenario),
        ("start_of_replication", &triggers.start_of_replication),
        ("end_of_replication", &triggers.end_of_replication),
    ];
    for (trigger, actions) in lists {
        for (position, action) in actions.iter().enumerate() {
            if let crate::action::Action::Script { source } = action {
                validate_script_source(
                    source,
                    &format!("model_triggers.{trigger}[{position}].source"),
                    &mut issues,
                );
            }
        }
    }
    issues
}

/// `#rrggbb` (or `#rgb`), the only colour form the GUI and SVG export accept.
fn is_hex_color(value: &str) -> bool {
    let hex = match value.strip_prefix('#') {
        Some(hex) => hex,
        None => return false,
    };
    matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit())
}

/// Objects that may take part in an S (center-port) connection.
fn is_center_port_object(kind: ObjectKind) -> bool {
    !matches!(kind, ObjectKind::Source | ObjectKind::Sink)
}

/// Validate every global table: legal names, non-empty unique columns and
/// rectangular rows.
fn validate_global_tables(model: &Model) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for (name, table) in &model.global_tables {
        let location = format!("global_tables.{name}");
        if !is_identifier(name) {
            issues.push(ValidationIssue::new(
                &location,
                "table names must start with a letter or underscore and contain only letters, digits and underscores",
            ));
        }
        if table.columns.is_empty() && !table.rows.is_empty() {
            issues.push(ValidationIssue::new(
                &location,
                "a table with rows needs column names",
            ));
        }
        let mut seen_columns = std::collections::BTreeMap::new();
        for (position, column) in table.columns.iter().enumerate() {
            if column.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{location}.columns[{position}]"),
                    "column names cannot be empty",
                ));
            }
            if seen_columns.insert(column, ()).is_some() {
                issues.push(ValidationIssue::new(
                    format!("{location}.columns[{position}]"),
                    format!("duplicate column name '{column}'"),
                ));
            }
        }
        for (position, row) in table.rows.iter().enumerate() {
            if row.len() != table.columns.len() {
                issues.push(ValidationIssue::new(
                    format!("{location}.rows[{position}]"),
                    format!(
                        "row has {} cells but the table has {} columns",
                        row.len(),
                        table.columns.len()
                    ),
                ));
            }
        }
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

/// Shared rhai engine used only for the model-level syntax check.
///
/// The sandbox API (`set_label`, ...) lives in `openflex-core::script`; rhai
/// resolves function calls at run time, so an empty engine still accepts those
/// names and the model layer stays free of a core dependency.
fn script_syntax_engine() -> &'static rhai::Engine {
    static ENGINE: std::sync::OnceLock<rhai::Engine> = std::sync::OnceLock::new();
    ENGINE.get_or_init(rhai::Engine::new)
}

fn validate_script_source(source: &str, location: &str, issues: &mut Vec<ValidationIssue>) {
    if source.trim().is_empty() {
        issues.push(ValidationIssue::new(
            location,
            "script source cannot be empty",
        ));
        return;
    }
    if let Err(error) = script_syntax_engine().compile(source) {
        issues.push(ValidationIssue::new(
            location,
            format!("script does not compile: {error}"),
        ));
    }
}

/// Check every conditional routing/decision rule: a rule may use an expression
/// condition or a script condition, but not both (the JSON parser rejects the
/// combination; this catches programmatic construction too) and the script must
/// compile.
fn validate_conditional_ports(
    conditions: &[crate::model::ConditionalPort],
    location: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    for (position, condition) in conditions.iter().enumerate() {
        let at = format!("{location}.conditions[{position}]");
        if let Some(script) = &condition.script_condition {
            if condition.condition != crate::expr::Expr::Bool(true) {
                issues.push(ValidationIssue::new(
                    format!("{at}.condition"),
                    "condition and script_condition are mutually exclusive",
                ));
            }
            validate_script_source(script, &format!("{at}.script_condition"), issues);
        }
    }
}

/// Check `script` trigger actions: non-empty source and compilable syntax.
fn validate_trigger_scripts(object: &Object) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
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
    ];
    for (trigger, actions) in lists {
        for (position, action) in actions.iter().enumerate() {
            if let crate::action::Action::Script { source } = action {
                validate_script_source(
                    source,
                    &format!(
                        "objects.{}.triggers.{trigger}[{position}].source",
                        object.name
                    ),
                    &mut issues,
                );
            }
        }
    }
    issues
}

/// Validate a Process Flow graph (B7), either model-level (`owner = None`) or
/// bound to one object.
fn validate_process_flow(
    model: &Model,
    flow: Option<&crate::model::ProcessFlow>,
    owner: Option<&Object>,
) -> Vec<ValidationIssue> {
    use crate::model::FlowBlockKind;

    let Some(flow) = flow else {
        return Vec::new();
    };
    let object_index: std::collections::HashMap<&str, usize> = model
        .objects
        .iter()
        .enumerate()
        .map(|(index, object)| (object.name.as_str(), index))
        .collect();
    let mut issues = Vec::new();
    if flow.blocks.is_empty() {
        issues.push(ValidationIssue::new(
            "process_flow.blocks",
            "a process flow needs at least one block",
        ));
        return issues;
    }

    // Unique, non-empty ids.
    let mut index: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for (position, block) in flow.blocks.iter().enumerate() {
        if block.id.trim().is_empty() {
            issues.push(ValidationIssue::new(
                format!("process_flow.blocks[{position}].id"),
                "block ids cannot be empty",
            ));
        } else if index.insert(block.id.as_str(), position).is_some() {
            issues.push(ValidationIssue::new(
                format!("process_flow.blocks.{}", block.id),
                "duplicate block id",
            ));
        }
    }

    // Resolve connections and collect the outgoing ports per block.
    let mut outgoing: Vec<Vec<u32>> = vec![Vec::new(); flow.blocks.len()];
    for (position, connection) in flow.connections.iter().enumerate() {
        let location = format!("process_flow.connections[{position}]");
        let Some(&from) = index.get(connection.from.as_str()) else {
            issues.push(ValidationIssue::new(
                &location,
                format!("unknown source block '{}'", connection.from),
            ));
            continue;
        };
        let Some(&to) = index.get(connection.to.as_str()) else {
            issues.push(ValidationIssue::new(
                &location,
                format!("unknown target block '{}'", connection.to),
            ));
            continue;
        };
        if from == to {
            issues.push(ValidationIssue::new(
                &location,
                "a block cannot connect to itself",
            ));
        }
        let port = connection.port.unwrap_or(outgoing[from].len() as u32);
        outgoing[from].push(port);
    }

    // Per-block constraints.
    for (position, block) in flow.blocks.iter().enumerate() {
        let location = format!("process_flow.blocks.{}", block.id);
        match &block.kind {
            FlowBlockKind::Create(params) => {
                if params.quantity == 0 {
                    issues.push(ValidationIssue::new(
                        format!("{location}.quantity"),
                        "must be at least 1",
                    ));
                }
                if let Err(err) = params.interarrival.validate() {
                    issues.push(ValidationIssue::new(
                        format!("{location}.interarrival"),
                        err,
                    ));
                }
                let zero_interval = !params.on_entry
                    && matches!(
                        params.interarrival,
                        crate::distribution::Distribution::Constant { value } if value <= 0.0
                    );
                if zero_interval && params.max_creations.is_none() {
                    issues.push(ValidationIssue::new(
                        format!("{location}.interarrival"),
                        "a zero interval without max_creations would create tokens forever",
                    ));
                }
                if params.on_entry && owner.is_none() {
                    issues.push(ValidationIssue::new(
                        &location,
                        "on_entry create blocks need an object-level process flow",
                    ));
                }
                if outgoing[position].len() != 1 {
                    issues.push(ValidationIssue::new(
                        &location,
                        "a create block needs exactly one outgoing connection",
                    ));
                }
            }
            FlowBlockKind::Delay(params) => {
                if let Err(err) = params.time.validate() {
                    issues.push(ValidationIssue::new(format!("{location}.time"), err));
                }
                if outgoing[position].len() != 1 {
                    issues.push(ValidationIssue::new(
                        &location,
                        "a delay block needs exactly one outgoing connection",
                    ));
                }
            }
            FlowBlockKind::Assign(params) => {
                for (action_position, action) in params.actions.iter().enumerate() {
                    if !matches!(
                        action,
                        crate::action::Action::SetLabel { .. }
                            | crate::action::Action::SetItemType { .. }
                            | crate::action::Action::IncrementLabel { .. }
                    ) {
                        issues.push(ValidationIssue::new(
                            format!("{location}.actions[{action_position}]"),
                            "assign blocks only support set_label, set_item_type and increment_label",
                        ));
                    }
                }
                if outgoing[position].len() != 1 {
                    issues.push(ValidationIssue::new(
                        &location,
                        "an assign block needs exactly one outgoing connection",
                    ));
                }
            }
            FlowBlockKind::Decision(params) => {
                if outgoing[position].is_empty() {
                    issues.push(ValidationIssue::new(
                        &location,
                        "a decision block needs at least one outgoing connection",
                    ));
                }
                for (condition_position, condition) in params.conditions.iter().enumerate() {
                    if !outgoing[position].contains(&condition.port) {
                        issues.push(ValidationIssue::new(
                            format!("{location}.conditions[{condition_position}].port"),
                            format!("no outgoing connection uses port {}", condition.port),
                        ));
                    }
                }
                validate_conditional_ports(&params.conditions, &location, &mut issues);
            }
            FlowBlockKind::Sink => {
                if !outgoing[position].is_empty() {
                    issues.push(ValidationIssue::new(
                        &location,
                        "a sink block has no outgoing connections",
                    ));
                }
            }
            FlowBlockKind::Transfer(params) => match object_index
                .get(params.object.as_str())
                .map(|&index| &model.objects[index])
            {
                None => issues.push(ValidationIssue::new(
                    &location,
                    format!("transfer target '{}' is unknown", params.object),
                )),
                Some(target) => match params.mode {
                    crate::model::TransferMode::ToObject
                        if target.kind == ObjectKind::Source
                            || matches!(
                                target.kind,
                                ObjectKind::FluidGenerator
                                    | ObjectKind::FluidTank
                                    | ObjectKind::FluidValve
                                    | ObjectKind::FluidSink
                                    | ObjectKind::FluidPipe
                            ) =>
                    {
                        issues.push(ValidationIssue::new(
                            &location,
                            format!(
                                "transfer target '{}' cannot accept flow items",
                                params.object
                            ),
                        ));
                    }
                    crate::model::TransferMode::FromObject
                        if !matches!(target.kind, ObjectKind::Queue | ObjectKind::Rack) =>
                    {
                        issues.push(ValidationIssue::new(
                            &location,
                            format!(
                                "from_object transfer source '{}' must be a queue or rack",
                                params.object
                            ),
                        ));
                    }
                    crate::model::TransferMode::FromObject if outgoing[position].len() != 1 => {
                        issues.push(ValidationIssue::new(
                            &location,
                            "a from_object transfer needs exactly one outgoing connection",
                        ));
                    }
                    _ => {}
                },
            },
        }
    }

    // The graph must be acyclic so tokens cannot loop forever.
    if has_cycle(flow, &index) {
        issues.push(ValidationIssue::new(
            "process_flow",
            "the block graph must be acyclic",
        ));
    }
    if !flow
        .blocks
        .iter()
        .any(|block| matches!(block.kind, FlowBlockKind::Create(_)))
    {
        issues.push(ValidationIssue::new(
            "process_flow.blocks",
            "a process flow needs a create block to produce tokens",
        ));
    }
    let has_sink = flow
        .blocks
        .iter()
        .any(|block| matches!(block.kind, FlowBlockKind::Sink));
    let has_transfer = flow.blocks.iter().any(|block| {
        matches!(
            &block.kind,
            FlowBlockKind::Transfer(params) if params.mode == crate::model::TransferMode::ToObject
        )
    });
    if !has_sink && !has_transfer {
        issues.push(ValidationIssue::new(
            "process_flow.blocks",
            "a process flow needs a sink or a transfer block to complete tokens",
        ));
    }
    issues
}

/// Depth-first cycle detection over the resolved block graph.
fn has_cycle(
    flow: &crate::model::ProcessFlow,
    index: &std::collections::BTreeMap<&str, usize>,
) -> bool {
    let count = flow.blocks.len();
    let mut adjacency: Vec<Vec<usize>> = vec![Vec::new(); count];
    for connection in &flow.connections {
        let (Some(&from), Some(&to)) = (
            index.get(connection.from.as_str()),
            index.get(connection.to.as_str()),
        ) else {
            continue;
        };
        adjacency[from].push(to);
    }
    fn visit(node: usize, adjacency: &[Vec<usize>], state: &mut [u8]) -> bool {
        state[node] = 1;
        for &next in &adjacency[node] {
            if state[next] == 1 {
                return true;
            }
            if state[next] == 0 && visit(next, adjacency, state) {
                return true;
            }
        }
        state[node] = 2;
        false
    }
    let mut state = vec![0u8; count];
    (0..count).any(|node| state[node] == 0 && visit(node, &adjacency, &mut state))
}

/// Whether an object kind belongs to the fluid family.
///
/// The two bridges (`item_to_fluid`, `fluid_to_item`) are deliberately *not*
/// part of this set: they exist to connect the fluid and the discrete side, so
/// the "fluid and discrete cannot connect" rule must not reject them.
pub fn is_fluid_kind(kind: ObjectKind) -> bool {
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

/// Whether an object kind converts between the fluid and the discrete side.
pub fn is_fluid_bridge(kind: ObjectKind) -> bool {
    matches!(kind, ObjectKind::ItemToFluid | ObjectKind::FluidToItem)
}

/// Validate the fluid sub-graph (B5).
fn validate_fluid(model: &Model) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    if let Some(step) = model.fluid_step
        && (!step.is_finite() || step <= 0.0)
    {
        issues.push(ValidationIssue::new(
            "fluid_step",
            "must be a positive number of time units",
        ));
    }
    let fluid_objects: Vec<&Object> = model
        .objects
        .iter()
        .filter(|object| is_fluid_kind(object.kind))
        .collect();
    if fluid_objects.is_empty() {
        return issues;
    }
    let object_index: std::collections::HashMap<&str, usize> = model
        .objects
        .iter()
        .enumerate()
        .map(|(index, object)| (object.name.as_str(), index))
        .collect();

    let fluid_names: std::collections::BTreeSet<&str> = fluid_objects
        .iter()
        .map(|object| object.name.as_str())
        .collect();
    let mut adjacency: std::collections::BTreeMap<&str, Vec<&str>> =
        std::collections::BTreeMap::new();
    for connection in &model.connections {
        let from_fluid = fluid_names.contains(connection.from.as_str());
        let to_fluid = fluid_names.contains(connection.to.as_str());
        let location = format!("connections {} -> {}", connection.from, connection.to);
        // A bridge object may sit on either side of the boundary.
        let touches_bridge = model
            .object(&connection.from)
            .is_some_and(|object| is_fluid_bridge(object.kind))
            || model
                .object(&connection.to)
                .is_some_and(|object| is_fluid_bridge(object.kind));
        if from_fluid != to_fluid && !touches_bridge {
            issues.push(ValidationIssue::new(
                &location,
                "fluid and discrete objects cannot be connected to each other",
            ));
            continue;
        }
        if from_fluid != to_fluid {
            // A bridge connection: nothing further to check in the fluid graph.
            continue;
        }
        if !from_fluid {
            continue;
        }
        let Some(&from_index) = object_index.get(connection.from.as_str()) else {
            continue;
        };
        let Some(&to_index) = object_index.get(connection.to.as_str()) else {
            continue;
        };
        let from = &model.objects[from_index];
        let to = &model.objects[to_index];
        if let Some(rate) = connection.flow_rate
            && (!rate.is_finite() || rate <= 0.0)
        {
            issues.push(ValidationIssue::new(
                &location,
                "fluid flow_rate must be positive when present",
            ));
        }
        if from.kind == ObjectKind::FluidGenerator
            && model
                .inputs(&connection.from)
                .iter()
                .any(|link| fluid_names.contains(link.from.as_str()))
        {
            issues.push(ValidationIssue::new(
                format!("objects.{}", connection.from),
                "a fluid generator has no fluid inputs",
            ));
        }
        if to.kind == ObjectKind::FluidSink
            && model
                .outputs(&connection.to)
                .iter()
                .any(|link| fluid_names.contains(link.to.as_str()))
        {
            issues.push(ValidationIssue::new(
                format!("objects.{}", connection.to),
                "a fluid sink has no fluid outputs",
            ));
        }
        adjacency
            .entry(connection.from.as_str())
            .or_default()
            .push(connection.to.as_str());
    }

    // Pipe graphs must be acyclic so a fixed-step update has a topological order.
    fn visit<'a>(
        node: &'a str,
        adjacency: &std::collections::BTreeMap<&'a str, Vec<&'a str>>,
        state: &mut std::collections::BTreeMap<&'a str, u8>,
    ) -> bool {
        state.insert(node, 1);
        if let Some(next_nodes) = adjacency.get(node) {
            for &next in next_nodes {
                let next_state = state.get(next).copied().unwrap_or(0);
                if next_state == 1 {
                    return true;
                }
                if next_state == 0 && visit(next, adjacency, state) {
                    return true;
                }
            }
        }
        state.insert(node, 2);
        false
    }
    let mut state = std::collections::BTreeMap::new();
    if fluid_names.iter().any(|name| {
        state.get(name).copied().unwrap_or(0) == 0 && visit(name, &adjacency, &mut state)
    }) {
        issues.push(ValidationIssue::new(
            "connections",
            "the fluid pipe graph must be acyclic",
        ));
    }
    issues
}

/// Validate global variables: legal names and a value matching the declared
/// kind (FlexSim `ModelingTools/GlobalVariables`).
fn validate_global_variables(model: &Model) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for (name, variable) in &model.global_variables {
        let location = format!("global_variables.{name}");
        if !is_identifier(name) {
            issues.push(ValidationIssue::new(
                &location,
                "global variable names must start with a letter or underscore and contain only letters, digits and underscores",
            ));
        }
        match variable.kind {
            crate::model::GlobalVariableKind::Number => match variable.value.as_f64() {
                Some(value) if value.is_finite() => {}
                Some(_) => issues.push(ValidationIssue::new(
                    format!("{location}.value"),
                    "a number global variable must be finite",
                )),
                None => issues.push(ValidationIssue::new(
                    format!("{location}.value"),
                    format!(
                        "kind is 'number' but the value is {}; use a JSON number",
                        json_kind(&variable.value)
                    ),
                )),
            },
            crate::model::GlobalVariableKind::String => {
                if variable.value.as_str().is_none() {
                    issues.push(ValidationIssue::new(
                        format!("{location}.value"),
                        format!(
                            "kind is 'string' but the value is {}; use a JSON string",
                            json_kind(&variable.value)
                        ),
                    ));
                }
            }
        }
    }
    issues
}

/// Human readable JSON type of a value, used in validation messages.
fn json_kind(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "a boolean",
        serde_json::Value::Number(_) => "a number",
        serde_json::Value::String(_) => "a string",
        serde_json::Value::Array(_) => "an array",
        serde_json::Value::Object(_) => "an object",
    }
}

/// Validate pick lists (FlexSim `PickLists/`): legal names, at least one entry,
/// non-empty unique entry names.
fn validate_picklists(model: &Model) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    for (name, picklist) in &model.picklists {
        let location = format!("picklists.{name}");
        if !is_identifier(name) {
            issues.push(ValidationIssue::new(
                &location,
                "pick list names must start with a letter or underscore and contain only letters, digits and underscores",
            ));
        }
        if picklist.entries.is_empty() {
            issues.push(ValidationIssue::new(
                &location,
                "a pick list needs at least one entry",
            ));
        }
        let mut seen = std::collections::BTreeMap::new();
        for (position, entry) in picklist.entries.iter().enumerate() {
            if entry.name.trim().is_empty() {
                issues.push(ValidationIssue::new(
                    format!("{location}.entries[{position}].name"),
                    "entry names cannot be empty",
                ));
            }
            if seen.insert(entry.name.as_str(), ()).is_some() {
                issues.push(ValidationIssue::new(
                    format!("{location}.entries[{position}].name"),
                    format!("duplicate entry name '{}'", entry.name),
                ));
            }
        }
    }
    issues
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
                flow_rate: None,
                strategy: None,
                kind: None,
            }],
            experiments: Vec::new(),
            process_flow: None,
            fluid_step: None,
            grid: None,
            global_tables: Default::default(),
            global_variables: Default::default(),
            picklists: Default::default(),
            model_triggers: Default::default(),
            performance_measures: Vec::new(),
            tracked_variables: Vec::new(),
            tracked_interval: None,
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
            flow_rate: None,
            strategy: None,
            kind: None,
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
            flow_rate: None,
            strategy: Some(PortStrategy::ByPercentage {
                percentages: vec![50.0, 40.0],
            }),
            kind: None,
        });
        model.connections.push(Connection {
            from: "Source1".into(),
            to: "Sink2".into(),
            port: None,
            flow_rate: None,
            strategy: None,
            kind: None,
        });
        let issues = validate_collect(&model);
        assert!(
            issues
                .iter()
                .any(|issue| issue.message.contains("sum to 100"))
        );
    }

    /// task-155: performance measures need identifier/unique names and a
    /// compileable expression; the location names the entry.
    #[test]
    fn performance_measures_validate_names_and_expressions() {
        let mut model = base_model();
        model.performance_measures = vec![
            PerformanceMeasure {
                name: "Bad Name".into(),
                expression: "1 +".into(),
                unit: None,
            },
            PerformanceMeasure {
                name: "Dup".into(),
                expression: "completed".into(),
                unit: Some("items".into()),
            },
            PerformanceMeasure {
                name: "Dup".into(),
                expression: "created".into(),
                unit: None,
            },
        ];
        let issues = validate_collect(&model);
        assert!(
            issues
                .iter()
                .any(|issue| issue.location == "performance_measures[0].name"),
            "{issues:?}"
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.location == "performance_measures[0].expression"),
            "{issues:?}"
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.location == "performance_measures[2].name"
                    && issue.message.contains("duplicate")),
            "{issues:?}"
        );

        model.performance_measures = vec![PerformanceMeasure {
            name: "WIP".into(),
            expression: "completed".into(),
            unit: Some("items".into()),
        }];
        assert!(validate_collect(&model).is_empty());
    }
}
