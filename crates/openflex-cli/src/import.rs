//! CSV import for `openflex import`.
//!
//! Three tables are understood; at least one of them must be given:
//!
//! - `--csv arrivals.csv` — `time,quantity[,item_type]`, optional header.
//!   Produces a table-driven `arrival_schedule` (or feeds the source of a
//!   structured model).
//! - `--objects objects.csv` — `name,kind,pos_x,pos_y`, optional header.
//! - `--connections connections.csv` — `from,to,port,strategy`, optional
//!   header. `strategy` is either a short name (`first_available`,
//!   `round_robin`, `random`, `by_item_type`, `hold`) or a JSON object such as
//!   `{"type":"by_percentage","percentages":[60,40]}` (quote the field in the
//!   CSV because it contains commas).
//!
//! Without an arrival table the source gets a safe default (constant
//! inter-arrival time 5) instead of the engine default of 0, which would create
//! items at the same instant forever. When no objects are supplied, a minimal
//! `Source1 -> Sink1` model is generated exactly as before. When objects are
//! supplied without a Source, one is inserted; without connections the objects
//! are chained in file order.
//!
//! Every generated model is parsed through [`openflex_model::Model::from_json`]
//! before it is written, so `openflex import` can never emit a model the engine
//! rejects.

use std::collections::BTreeSet;

use openflex_model::{Model, ObjectKind};
use serde_json::{Value, json};

/// One parsed arrival row.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrival {
    pub time: f64,
    pub quantity: u32,
    pub item_type: Option<u32>,
}

/// One object row from `objects.csv`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedObject {
    pub name: String,
    pub kind: ObjectKind,
    pub pos: [f64; 2],
}

/// One connection row from `connections.csv`.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedConnection {
    pub from: String,
    pub to: String,
    pub port: Option<u32>,
    pub strategy: Option<Value>,
}

/// Parse the contents of an arrivals CSV (`time,quantity[,item_type]`).
pub fn parse_csv(text: &str) -> Result<Vec<Arrival>, String> {
    let mut rows = Vec::new();
    let mut first_data_line = true;
    for (number, line) in lines(text) {
        let fields = fields(line);
        let time = match fields.first().map(|field| field.parse::<f64>()) {
            Some(Ok(time)) => time,
            Some(Err(_)) if first_data_line => {
                // A header row: accept it if the first column looks like a
                // label rather than a number.
                first_data_line = false;
                continue;
            }
            Some(Err(error)) => {
                return Err(format!("line {}: invalid time: {error}", number + 1));
            }
            None => return Err(format!("line {}: expected time,quantity", number + 1)),
        };
        first_data_line = false;
        if !time.is_finite() || time < 0.0 {
            return Err(format!(
                "line {}: arrival time must be a non-negative number, found {time}",
                number + 1
            ));
        }
        let quantity = match fields.get(1).map(String::as_str) {
            None | Some("") => 1,
            Some(field) => field.parse::<u32>().map_err(|error| {
                format!("line {}: invalid quantity '{field}': {error}", number + 1)
            })?,
        };
        let item_type = match fields.get(2).map(String::as_str) {
            None | Some("") => None,
            Some(field) => Some(field.parse::<u32>().map_err(|error| {
                format!("line {}: invalid item_type '{field}': {error}", number + 1)
            })?),
        };
        rows.push(Arrival {
            time,
            quantity,
            item_type,
        });
    }
    if rows.is_empty() {
        return Err("no arrival rows found (expected at least time,quantity)".into());
    }
    rows.sort_by(|left, right| left.time.total_cmp(&right.time));
    Ok(rows)
}

/// Parse `objects.csv` (`name,kind,pos_x,pos_y`, optional header).
pub fn parse_objects_csv(text: &str) -> Result<Vec<ImportedObject>, String> {
    let mut objects = Vec::new();
    let mut names = BTreeSet::new();
    for (number, line) in lines(text) {
        let fields = fields(line);
        if fields.len() >= 2
            && fields[0].eq_ignore_ascii_case("name")
            && fields[1].eq_ignore_ascii_case("kind")
        {
            continue;
        }
        let name = fields.first().map(String::as_str).unwrap_or_default();
        if name.is_empty() {
            return Err(format!("line {}: object name is empty", number + 1));
        }
        if !names.insert(name.to_string()) {
            return Err(format!(
                "line {}: duplicate object name '{name}'",
                number + 1
            ));
        }
        let kind_text = fields
            .get(1)
            .map(String::as_str)
            .filter(|kind| !kind.is_empty())
            .ok_or_else(|| format!("line {}: missing object kind for '{name}'", number + 1))?;
        let normalized = kind_text.to_ascii_lowercase();
        let kind = ObjectKind::parse(&normalized).ok_or_else(|| {
            format!(
                "line {}: unknown object kind '{kind_text}' for '{name}'",
                number + 1
            )
        })?;
        let pos_x = coordinate(fields.get(2).map(String::as_str), number, "pos_x")?;
        let pos_y = coordinate(fields.get(3).map(String::as_str), number, "pos_y")?;
        objects.push(ImportedObject {
            name: name.to_string(),
            kind,
            pos: [pos_x, pos_y],
        });
    }
    if objects.is_empty() {
        return Err("no object rows found (expected name,kind[,pos_x,pos_y])".into());
    }
    Ok(objects)
}

/// Parse `connections.csv` (`from,to[,port,strategy]`, optional header).
pub fn parse_connections_csv(text: &str) -> Result<Vec<ImportedConnection>, String> {
    let mut connections = Vec::new();
    for (number, line) in lines(text) {
        let fields = fields(line);
        if fields.len() >= 2
            && fields[0].eq_ignore_ascii_case("from")
            && fields[1].eq_ignore_ascii_case("to")
        {
            continue;
        }
        let from = fields.first().map(String::as_str).unwrap_or_default();
        let to = fields.get(1).map(String::as_str).unwrap_or_default();
        if from.is_empty() || to.is_empty() {
            return Err(format!(
                "line {}: connections need both from and to",
                number + 1
            ));
        }
        let port =
            match fields
                .get(2)
                .map(String::as_str)
                .filter(|port| !port.is_empty())
            {
                None => None,
                Some(port) => Some(port.parse::<u32>().map_err(|error| {
                    format!("line {}: invalid port '{port}': {error}", number + 1)
                })?),
            };
        let strategy = match fields
            .get(3)
            .map(String::as_str)
            .filter(|strategy| !strategy.is_empty())
        {
            None => None,
            Some(strategy) => Some(
                parse_strategy(strategy)
                    .map_err(|error| format!("line {}: invalid strategy: {error}", number + 1))?,
            ),
        };
        connections.push(ImportedConnection {
            from: from.to_string(),
            to: to.to_string(),
            port,
            strategy,
        });
    }
    if connections.is_empty() {
        return Err("no connection rows found (expected from,to[,port,strategy])".into());
    }
    Ok(connections)
}

fn parse_strategy(text: &str) -> Result<Value, String> {
    if text.trim_start().starts_with('{') {
        return serde_json::from_str::<Value>(text).map_err(|error| error.to_string());
    }
    let normalized = text.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "first_available" | "round_robin" | "random" | "by_item_type" | "hold" => {
            Ok(json!({ "type": normalized }))
        }
        other => Err(format!(
            "unknown strategy '{other}' (expected first_available, round_robin, random, by_item_type, hold or a JSON object)"
        )),
    }
}

/// Build the JSON text of a model from the imported tables.
///
/// Returns the JSON (with a trailing newline) and the stop time written into
/// the model.
pub fn build_model(
    name: &str,
    unit: &str,
    arrivals: &[Arrival],
    objects: &[ImportedObject],
    connections: &[ImportedConnection],
) -> Result<(String, f64), String> {
    let mut object_values: Vec<Value> = objects
        .iter()
        .map(|object| {
            json!({
                "name": object.name,
                "kind": object.kind.as_str(),
                "pos": [object.pos[0], object.pos[1]],
                "params": {},
            })
        })
        .collect();
    let mut stop_time = 1000.0_f64;
    let mut description = format!(
        "Imported from CSV: {} objects, {} connections.",
        objects.len(),
        connections.len()
    );

    // Make sure the model has a source; without an arrival table it gets a
    // safe non-zero inter-arrival time so the run cannot spin at t = 0.
    let source_name = match objects
        .iter()
        .find(|object| object.kind == ObjectKind::Source)
    {
        Some(source) => source.name.clone(),
        None => {
            // Keep the generated source first so the implicit chain
            // (`--connections` omitted) flows Source -> objects in file order.
            let default_source = unique_name(&object_values, "Source1");
            let position = if object_values.is_empty() {
                [120.0, 200.0]
            } else {
                [-100.0, 0.0]
            };
            object_values.insert(
                0,
                json!({
                    "name": default_source,
                    "kind": "source",
                    "pos": position,
                    "params": {},
                }),
            );
            default_source.to_string()
        }
    };

    let schedule_rows: Vec<Value> = arrivals
        .iter()
        .map(|row| {
            let mut value = json!({
                "time": row.time,
                "quantity": row.quantity,
            });
            if let Some(item_type) = row.item_type {
                value["item_type"] = json!(item_type);
            }
            value
        })
        .collect();
    let total_quantity: u64 = arrivals.iter().map(|row| row.quantity as u64).sum();

    if arrivals.is_empty() {
        // The engine default (constant 0) would create items at the same
        // instant forever; a constant 5 is the importer's safe default.
        set_source_params(
            &mut object_values,
            &source_name,
            json!({ "interarrival": { "distribution": "constant", "value": 5.0 } }),
        )?;
    } else {
        let first = arrivals.first().map(|row| row.time).unwrap_or(0.0);
        let last = arrivals.last().map(|row| row.time).unwrap_or(0.0);
        let mean_gap = if arrivals.len() > 1 {
            (last - first) / (arrivals.len() - 1) as f64
        } else {
            0.0
        };
        stop_time = (last + mean_gap.max(10.0)).max(10.0);
        set_source_params(
            &mut object_values,
            &source_name,
            json!({
                "arrival_schedule": { "rows": schedule_rows, "repeat": false },
                "max_arrivals": total_quantity,
            }),
        )?;
        description = format!(
            "Imported from CSV: {} arrival rows ({} items) from t={} to t={}, \
             {} objects, {} connections.",
            arrivals.len(),
            total_quantity,
            first,
            last,
            objects.len(),
            connections.len()
        );
    }

    // A sink is mandatory for a valid model. Pure arrival imports get the
    // default one; a structure import must say where items leave the system.
    if objects.is_empty() {
        object_values.push(json!({
            "name": "Sink1", "kind": "sink", "pos": [520.0, 200.0], "params": {},
        }));
    } else if !objects.iter().any(|object| object.kind == ObjectKind::Sink) {
        return Err("objects table needs at least one sink object".into());
    }

    // Connections: validate the explicit table, or chain the objects in file
    // order so a structure-only import is runnable without extra work.
    let names: BTreeSet<&str> = object_values
        .iter()
        .filter_map(|object| object["name"].as_str())
        .collect();
    let connection_values: Vec<Value> = if connections.is_empty() {
        let mut generated = Vec::new();
        let ordered: Vec<&str> = object_values
            .iter()
            .filter_map(|object| object["name"].as_str())
            .collect();
        for pair in ordered.windows(2) {
            generated.push(json!({ "from": pair[0], "to": pair[1] }));
        }
        generated
    } else {
        let mut explicit = Vec::with_capacity(connections.len());
        for connection in connections {
            if !names.contains(connection.from.as_str()) {
                return Err(format!(
                    "connection from '{}' refers to an unknown object",
                    connection.from
                ));
            }
            if !names.contains(connection.to.as_str()) {
                return Err(format!(
                    "connection to '{}' refers to an unknown object",
                    connection.to
                ));
            }
            let mut value = json!({ "from": connection.from, "to": connection.to });
            if let Some(port) = connection.port {
                value["port"] = json!(port);
            }
            if let Some(strategy) = &connection.strategy {
                value["strategy"] = strategy.clone();
            }
            explicit.push(value);
        }
        explicit
    };

    let mut model = json!({
        "format": "openflex-model",
        "version": 1,
        "name": name,
        "description": description,
        "time_unit": unit,
        "run": { "stop_time": stop_time, "warmup": 0.0, "seed": 1, "replications": 1 },
        "objects": object_values,
        "connections": connection_values,
        "metadata": {
            "imported_from": "csv",
            "object_rows": objects.len(),
            "connection_rows": connections.len(),
            "arrival_rows": arrivals.len(),
        }
    });
    if !schedule_rows.is_empty() {
        model["metadata"]["arrivals"] = Value::Array(schedule_rows);
    }
    let text = serde_json::to_string_pretty(&model).map_err(|error| error.to_string())?;
    // Parse through the real model loader: the importer must never write a
    // model the engine would reject.
    Model::from_json(&text).map_err(|error| error.to_string())?;
    Ok((text + "\n", stop_time))
}

/// Return `preferred` or `preferred-N`, avoiding an existing object name.
fn unique_name(objects: &[Value], preferred: &str) -> String {
    let names: BTreeSet<&str> = objects
        .iter()
        .filter_map(|object| object["name"].as_str())
        .collect();
    if !names.contains(preferred) {
        return preferred.to_string();
    }
    for suffix in 2.. {
        let candidate = format!("{preferred}-{suffix}");
        if !names.contains(candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!("an unused suffix always exists")
}

fn set_source_params(
    objects: &mut [Value],
    source_name: &str,
    params: Value,
) -> Result<(), String> {
    let source = objects
        .iter_mut()
        .find(|object| object["name"] == source_name)
        .ok_or_else(|| format!("source '{source_name}' is missing"))?;
    source["params"] = params;
    Ok(())
}

/// Yield `(line number, trimmed line)` with comments, blanks and BOM skipped.
fn lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .map(|line| line.trim().trim_start_matches('\u{feff}'))
        .enumerate()
        .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
}

/// Split one CSV line, honouring double-quoted fields (`""` escapes a quote).
///
/// This is intentionally small: imports are hand-written tables, not full RFC
/// 4180 documents. It is enough for a quoted JSON strategy such as
/// `"{\"type\":\"by_percentage\",\"percentages\":[60,40]}"`.
fn fields(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted => {
                if chars.peek() == Some(&'"') {
                    current.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            }
            '"' if !quoted && current.trim().is_empty() => {
                current.clear();
                quoted = true;
            }
            ',' if !quoted => {
                fields.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

fn coordinate(value: Option<&str>, line: usize, label: &str) -> Result<f64, String> {
    match value.filter(|value| !value.is_empty()) {
        None => Ok(0.0),
        Some(value) => value
            .parse::<f64>()
            .map_err(|error| format!("line {}: invalid {label} '{value}': {error}", line + 1)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header_and_ignores_comments_and_blank_lines() {
        let rows = parse_csv(
            "# exported from a spreadsheet\n\ntime,quantity,item_type\n0,1,0\n4.5,2,1\n9,1,\n",
        )
        .expect("parse");
        assert_eq!(rows.len(), 3);
        assert_eq!(
            rows[0],
            Arrival {
                time: 0.0,
                quantity: 1,
                item_type: Some(0)
            }
        );
        assert_eq!(rows[1].quantity, 2);
        assert_eq!(rows[2].item_type, None);
    }

    #[test]
    fn sorts_rows_by_time() {
        let rows = parse_csv("10,1\n2,3\n5,2\n").expect("parse");
        assert_eq!(
            rows.iter().map(|row| row.time).collect::<Vec<_>>(),
            vec![2.0, 5.0, 10.0]
        );
    }

    #[test]
    fn rejects_negative_and_malformed_times() {
        assert!(parse_csv("-1,1\n").is_err());
        assert!(parse_csv("soon,1\n").is_err());
        assert!(parse_csv("1,2\n").is_ok());
        assert!(parse_csv("").is_err());
    }

    #[test]
    fn parses_objects_and_connections_with_optional_headers() {
        let objects = parse_objects_csv(
            "name,kind,pos_x,pos_y\nSource,source,-100,0\nQueue1,queue,0,0\nExit,sink,100,0\n",
        )
        .expect("objects");
        assert_eq!(objects.len(), 3);
        assert_eq!(objects[1].name, "Queue1");
        assert_eq!(objects[1].kind, ObjectKind::Queue);
        assert_eq!(objects[0].pos, [-100.0, 0.0]);

        let connections = parse_connections_csv(
            "from,to,port,strategy\nSource,Queue1,0,round_robin\nQueue1,Exit,,\n",
        )
        .expect("connections");
        assert_eq!(connections.len(), 2);
        assert_eq!(connections[0].port, Some(0));
        assert_eq!(
            connections[0].strategy,
            Some(json!({ "type": "round_robin" }))
        );
        assert_eq!(connections[1].strategy, None);
    }

    #[test]
    fn connection_strategy_accepts_a_quoted_json_object() {
        let connections = parse_connections_csv(
            "from,to\nA,B,0,\"{\"\"type\"\":\"\"by_percentage\"\",\"\"percentages\"\":[60,40]}\",\n",
        )
        .expect("connections");
        assert_eq!(
            connections[0].strategy,
            Some(json!({ "type": "by_percentage", "percentages": [60, 40] }))
        );
    }

    #[test]
    fn object_and_connection_errors_name_the_line() {
        assert!(parse_objects_csv("A,cat\n").is_err());
        assert!(parse_objects_csv("A,queue\nA,sink\n").is_err());
        assert!(parse_connections_csv("A\n").is_err());
        assert!(parse_connections_csv("A,B,0,teleport\n").is_err());
    }

    #[test]
    fn generated_model_round_trips_through_the_model_loader() {
        let rows = parse_csv("0,1\n2,2\n4,1\n").expect("parse");
        let (text, stop_time) = build_model("demo", "seconds", &rows, &[], &[]).expect("model");
        assert!(stop_time > 4.0);
        let model = Model::from_json(&text).expect("valid model");
        assert_eq!(model.name, "demo");
        assert_eq!(model.objects.len(), 2);
        let source = model.object("Source1").expect("source");
        match &source.params {
            openflex_model::ObjectParams::Source(params) => assert!(
                params.arrival_schedule().is_some(),
                "the importer must use a table-driven source"
            ),
            other => panic!("Source1 is not a source: {other:?}"),
        }
    }

    #[test]
    fn structured_import_keeps_objects_and_connections() {
        let objects = parse_objects_csv(
            "name,kind,pos_x,pos_y\nIn,source,-100,0\nBuffer,queue,0,0\nServer,processor,100,0\nOut,sink,200,0\n",
        )
        .expect("objects");
        let connections = parse_connections_csv("from,to\nIn,Buffer\nBuffer,Server\nServer,Out\n")
            .expect("connections");
        let (text, _) = build_model("line", "seconds", &[], &objects, &connections).expect("model");
        let model = Model::from_json(&text).expect("valid model");
        assert_eq!(model.objects.len(), 4);
        assert_eq!(model.connections.len(), 3);
        assert_eq!(model.connections[0].from, "In");
    }

    #[test]
    fn a_structure_without_a_source_gets_a_default_one() {
        let objects = parse_objects_csv("Buffer,queue,0,0\nOut,sink,100,0\n").expect("objects");
        let connections = parse_connections_csv("Buffer,Out\n").expect("connections");
        let (text, _) =
            build_model("defaults", "seconds", &[], &objects, &connections).expect("model");
        let model = Model::from_json(&text).expect("valid model");
        let source = model.object("Source1").expect("default source");
        assert_eq!(source.kind, ObjectKind::Source);
        assert_eq!(model.connections.len(), 1, "explicit connections are kept");

        // Without --connections the objects are chained, with the generated
        // source first.
        let (text, _) = build_model("chained", "seconds", &[], &objects, &[]).expect("model");
        let model = Model::from_json(&text).expect("valid model");
        assert_eq!(model.connections.len(), 2);
        assert_eq!(model.connections[0].from, "Source1");
        assert_eq!(model.connections[0].to, "Buffer");
    }
}
