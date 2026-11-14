//! Static SVG layout export for `openflex layout`.
//!
//! OpenFlex has no 3D view yet, so this is the smallest useful visualisation:
//! every object is a rounded rectangle at its model `pos`, labelled with its
//! name and kind, and every connection is an arrow from the output port of the
//! source to the input port of the destination. Objects without positions are
//! laid out on a grid.
//!
//! The SVG is written by hand — a few hundred bytes of text — so the command
//! works everywhere without a graphics stack, and the result is diffable in a
//! pull request.

use std::fmt::Write as _;

use openflex_model::{Model, ObjectKind};

const NODE_WIDTH: f64 = 132.0;
const NODE_HEIGHT: f64 = 60.0;
const MARGIN: f64 = 48.0;

/// Render `model` as an SVG document of the requested size.
pub fn to_svg(model: &Model, width: u32, height: u32) -> String {
    // Guard the geometry against tiny or silly window sizes.
    let width = width.clamp(320, 10_000) as f64;
    let height = height.clamp(240, 10_000) as f64;
    let centers = centers(model, width, height);

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-label="OpenFlex model layout: {}">"#,
        escape(&model.name)
    );
    let _ = writeln!(
        svg,
        r##"<defs>
  <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 0 0 L 10 5 L 0 10 z" fill="#4b5563"/>
  </marker>
  <style>
    .canvas {{ fill: #f8fafc; }}
    .title {{ font: 600 18px system-ui, sans-serif; fill: #0f172a; }}
    .subtitle {{ font: 12px system-ui, sans-serif; fill: #64748b; }}
    .node {{ fill: #ffffff; stroke: #334155; stroke-width: 1.5; }}
    .node.source {{ stroke: #15803d; }}
    .node.sink {{ stroke: #b91c1c; }}
    .node.processor {{ stroke: #1d4ed8; }}
    .name {{ font: 600 13px system-ui, sans-serif; fill: #0f172a; }}
    .kind {{ font: 11px system-ui, sans-serif; fill: #64748b; }}
    .link {{ fill: none; stroke: #4b5563; stroke-width: 1.5; marker-end: url(#arrow); }}
    .port {{ fill: #94a3b8; }}
  </style>
</defs>"##
    );
    let _ = writeln!(
        svg,
        r#"<rect class="canvas" x="0" y="0" width="{width}" height="{height}"/>"#
    );
    let _ = writeln!(
        svg,
        r#"<text class="title" x="{MARGIN}" y="{title_y}">OpenFlex layout: {name}</text>"#,
        title_y = 32.0,
        name = escape(&model.name)
    );
    let _ = writeln!(
        svg,
        r#"<text class="subtitle" x="{MARGIN}" y="{subtitle_y}">{objects} objects, {connections} connections · {unit}</text>"#,
        subtitle_y = 50.0,
        objects = model.objects.len(),
        connections = model.connections.len(),
        unit = escape(model.time_unit.label())
    );

    // Connections first so the nodes cover the arrow heads where they meet.
    let mut links = String::new();
    for connection in &model.connections {
        let (Some(from), Some(to)) = (centers.get(&connection.from), centers.get(&connection.to))
        else {
            continue;
        };
        let start = (from.0 + NODE_WIDTH / 2.0, from.1);
        let end = (to.0 - NODE_WIDTH / 2.0, to.1);
        let _ = writeln!(
            links,
            r#"<path class="link" d="M {sx:.1} {sy:.1} C {cx1:.1} {sy:.1}, {cx2:.1} {ey:.1}, {ex:.1} {ey:.1}"/>"#,
            sx = start.0,
            sy = start.1,
            cx1 = start.0 + (end.0 - start.0).max(20.0) * 0.5,
            cx2 = end.0 - (end.0 - start.0).max(20.0) * 0.5,
            ex = end.0,
            ey = end.1
        );
    }
    let _ = writeln!(svg, r#"<g id="connections">"#);
    let _ = write!(svg, "{links}");
    let _ = writeln!(svg, "</g>");

    let _ = writeln!(svg, r#"<g id="objects">"#);
    for object in &model.objects {
        let Some((cx, cy)) = centers.get(&object.name) else {
            continue;
        };
        let x = cx - NODE_WIDTH / 2.0;
        let y = cy - NODE_HEIGHT / 2.0;
        let class = node_class(object.kind);
        let in_port = format!(
            "{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
            x,
            cy,
            x + 10.0,
            cy - 6.0,
            x + 10.0,
            cy + 6.0
        );
        let out_port = format!(
            "{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}",
            x + NODE_WIDTH,
            cy,
            x + NODE_WIDTH - 10.0,
            cy - 6.0,
            x + NODE_WIDTH - 10.0,
            cy + 6.0
        );
        let _ = writeln!(
            svg,
            r#"<g class="object" data-name="{name}">
  <rect class="node {class}" x="{x:.1}" y="{y:.1}" width="{NODE_WIDTH}" height="{NODE_HEIGHT}" rx="8"/>
  <polygon class="port" points="{in_port}"/>
  <polygon class="port" points="{out_port}"/>
  <text class="name" x="{tx:.1}" y="{ty:.1}" text-anchor="middle">{name}</text>
  <text class="kind" x="{tx:.1}" y="{ky:.1}" text-anchor="middle">{kind}</text>
</g>"#,
            name = escape(&object.name),
            class = class,
            x = x,
            y = y,
            in_port = in_port,
            out_port = out_port,
            tx = cx,
            ty = cy - 4.0,
            ky = cy + 14.0,
            kind = escape(object.kind.as_str())
        );
    }
    let _ = writeln!(svg, "</g>");
    svg.push_str("</svg>\n");
    svg
}

fn node_class(kind: ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Source => "source",
        ObjectKind::Sink => "sink",
        ObjectKind::Processor => "processor",
        _ => "",
    }
}

/// Map every object name to the centre of its node.
fn centers(
    model: &Model,
    width: f64,
    height: f64,
) -> std::collections::BTreeMap<String, (f64, f64)> {
    let mut centers = std::collections::BTreeMap::new();
    if model.objects.is_empty() {
        return centers;
    }

    let min_x = model
        .objects
        .iter()
        .map(|object| object.pos[0])
        .fold(f64::INFINITY, f64::min);
    let max_x = model
        .objects
        .iter()
        .map(|object| object.pos[0])
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = model
        .objects
        .iter()
        .map(|object| object.pos[1])
        .fold(f64::INFINITY, f64::min);
    let max_y = model
        .objects
        .iter()
        .map(|object| object.pos[1])
        .fold(f64::NEG_INFINITY, f64::max);

    let span_x = max_x - min_x;
    let span_y = max_y - min_y;
    let usable_w = (width - 2.0 * MARGIN - NODE_WIDTH).max(1.0);
    let usable_h = (height - 2.0 * MARGIN - NODE_HEIGHT - 40.0).max(1.0);

    if span_x.abs() < 1e-9 && span_y.abs() < 1e-9 {
        return grid(model, width, height);
    }

    let scale_x = if span_x.abs() < 1e-9 {
        0.0
    } else {
        usable_w / span_x
    };
    let scale_y = if span_y.abs() < 1e-9 {
        0.0
    } else {
        usable_h / span_y
    };

    for object in &model.objects {
        let x = MARGIN + NODE_WIDTH / 2.0 + (object.pos[0] - min_x) * scale_x;
        // Model coordinates grow upwards, screen coordinates grow downwards.
        let y = height - MARGIN - NODE_HEIGHT / 2.0 - (object.pos[1] - min_y) * scale_y;
        let x = if span_x.abs() < 1e-9 { width / 2.0 } else { x };
        let y = if span_y.abs() < 1e-9 { height / 2.0 } else { y };
        centers.insert(object.name.clone(), (x, y));
    }
    centers
}

/// Fallback layout when the model carries no distinct coordinates.
fn grid(model: &Model, width: f64, height: f64) -> std::collections::BTreeMap<String, (f64, f64)> {
    let mut centers = std::collections::BTreeMap::new();
    let count = model.objects.len();
    let columns = (count as f64).sqrt().ceil().max(1.0) as usize;
    let rows = count.div_ceil(columns).max(1);
    let cell_w = (width - 2.0 * MARGIN) / columns as f64;
    let cell_h = (height - 2.0 * MARGIN - 40.0) / rows as f64;
    for (index, object) in model.objects.iter().enumerate() {
        let column = index % columns;
        let row = index / columns;
        let x = MARGIN + cell_w * (column as f64 + 0.5);
        let y = 64.0 + MARGIN + cell_h * (row as f64 + 0.5);
        centers.insert(object.name.clone(), (x, y));
    }
    centers
}

/// Escape the five XML special characters so object names are always safe.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_xml_special_characters() {
        assert_eq!(escape("a<b&c"), "a&lt;b&amp;c");
    }

    #[test]
    fn a_model_without_positions_is_laid_out_on_a_grid() {
        let model = openflex_model::Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "layout-demo",
              "objects": [
                { "name": "Source1", "kind": "source" },
                { "name": "Queue1", "kind": "queue" },
                { "name": "Sink1", "kind": "sink" }
              ],
              "connections": [
                { "from": "Source1", "to": "Queue1" },
                { "from": "Queue1", "to": "Sink1" }
              ]
            }"#,
        )
        .expect("fixture model");
        let svg = to_svg(&model, 800, 600);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("Source1"));
        assert!(svg.contains("Queue1"));
        assert!(svg.contains("Sink1"));
        assert!(svg.contains("class=\"link\""));
        assert!(svg.trim_end().ends_with("</svg>"));
    }
}
