//! `openflex import --csv arrivals.csv --out model.ofm`
//!
//! Many real models start life as a spreadsheet column of arrival times. This
//! command turns such a table into the smallest legal OpenFlex model:
//! `Source1 -> Sink1`, with a table-driven `arrival_schedule` on the source
//! ([model arrival schedule](../openflex_model) semantics; absolute times,
//! optional quantity and item type per row).
//!
//! The CSV format is deliberately simple:
//!
//! ```csv
//! time,quantity,item_type
//! 0,1,0
//! 4.5,2,1
//! ```
//!
//! The header is optional. Blank lines and lines starting with `#` are
//! ignored, so annotated exports work. Times are model time units (seconds
//! unless `--time-unit` says otherwise) and must be non-negative; rows are
//! sorted by time before the model is written.

use openflex_model::Model;
use serde_json::{Value, json};

/// One parsed arrival row.
#[derive(Debug, Clone, PartialEq)]
pub struct Arrival {
    pub time: f64,
    pub quantity: u32,
    pub item_type: Option<u32>,
}

/// Parse the contents of an arrivals CSV.
pub fn parse_csv(text: &str) -> Result<Vec<Arrival>, String> {
    let mut rows = Vec::new();
    let mut first_data_line = true;
    for (number, line) in text.lines().enumerate() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let time = match fields.first().map(|field| field.parse::<f64>()) {
            Some(Ok(time)) => time,
            Some(Err(_)) if first_data_line && !fields.is_empty() => {
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
        let quantity = match fields.get(1) {
            None | Some(&"") => 1,
            Some(field) => field.parse::<u32>().map_err(|error| {
                format!("line {}: invalid quantity '{field}': {error}", number + 1)
            })?,
        };
        let item_type = match fields.get(2) {
            None | Some(&"") => None,
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

/// Build the JSON text of a `Source -> Sink` model from parsed arrivals.
///
/// `name` is the model name; `unit` is the `time_unit` string (`seconds`,
/// `minutes`, `hours`, `days`).
pub fn build_model(name: &str, unit: &str, rows: &[Arrival]) -> Result<(String, f64), String> {
    let first = rows.first().map(|row| row.time).unwrap_or(0.0);
    let last = rows.last().map(|row| row.time).unwrap_or(0.0);
    let mean_gap = if rows.len() > 1 {
        (last - first) / (rows.len() - 1) as f64
    } else {
        0.0
    };
    // Run long enough to consume the table and drain the sink.
    let stop_time = (last + mean_gap.max(10.0)).max(10.0);

    let schedule_rows: Vec<Value> = rows
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
    let total_quantity: u64 = rows.iter().map(|row| row.quantity as u64).sum();
    // `max_arrivals` is a safety cap: the arrival table is authoritative, but
    // engine builds that do not evaluate `arrival_schedule` yet would otherwise
    // fall back to the default zero inter-arrival time and never stop creating
    // items. With the cap the model always terminates (and, once schedules are
    // evaluated, the cap equals the number of items the table creates).
    let schedule = json!({
        "arrival_schedule": { "rows": schedule_rows, "repeat": false },
        "max_arrivals": total_quantity,
    });

    let model = json!({
        "format": "openflex-model",
        "version": 1,
        "name": name,
        "description": format!(
            "Imported from CSV: {} arrival rows from t={} to t={} ({} items).",
            rows.len(),
            first,
            last,
            total_quantity
        ),
        "time_unit": unit,
        "run": { "stop_time": stop_time, "warmup": 0.0, "seed": 1, "replications": 1 },
        "objects": [
            { "name": "Source1", "kind": "source", "pos": [120.0, 200.0], "params": schedule },
            { "name": "Sink1", "kind": "sink", "pos": [520.0, 200.0], "params": {} }
        ],
        "connections": [ { "from": "Source1", "to": "Sink1" } ],
        "metadata": {
            "imported_from": "csv",
            "arrival_rows": rows.len(),
            "arrivals": schedule_rows,
        }
    });
    let text = serde_json::to_string_pretty(&model).map_err(|error| error.to_string())?;
    // Parse through the real model loader: the importer must never write a
    // model the engine would reject.
    Model::from_json(&text).map_err(|error| error.to_string())?;
    Ok((text + "\n", stop_time))
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
    fn generated_model_round_trips_through_the_model_loader() {
        let rows = parse_csv("0,1\n2,2\n4,1\n").expect("parse");
        let (text, stop_time) = build_model("demo", "seconds", &rows).expect("model");
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
}
