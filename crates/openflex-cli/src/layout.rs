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

/// Optional animation frame for `--animate`.
#[derive(Debug, Clone, Copy)]
pub struct Frame {
    pub index: u32,
    pub count: u32,
}

impl Frame {
    /// Position of the moving items along each connection, in `[0, 1]`.
    fn progress(self) -> f64 {
        if self.count <= 1 {
            0.5
        } else {
            self.index as f64 / (self.count - 1) as f64
        }
    }
}

/// Flat 2D layout, optionally with animated item markers.
pub fn to_svg_flat(model: &Model, width: u32, height: u32, frame: Option<Frame>) -> String {
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
    .item {{ fill: #f59e0b; stroke: #b45309; stroke-width: 1; }}
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
        let rot = object.rot.unwrap_or(0.0);
        let _ = writeln!(
            svg,
            r#"<g class="object" data-name="{name}">
  <g transform="rotate({rot:.1} {cx:.1} {cy:.1})">
    <rect class="node {class}" x="{x:.1}" y="{y:.1}" width="{NODE_WIDTH}" height="{NODE_HEIGHT}" rx="8"/>
    <polygon class="port" points="{in_port}"/>
    <polygon class="port" points="{out_port}"/>
  </g>
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

    if let Some(frame) = frame {
        let progress = frame.progress();
        let _ = writeln!(svg, r#"<g id="items">"#);
        for connection in &model.connections {
            let (Some(from), Some(to)) =
                (centers.get(&connection.from), centers.get(&connection.to))
            else {
                continue;
            };
            let x = from.0 + (to.0 - from.0) * progress;
            let y = from.1 + (to.1 - from.1) * progress;
            let _ = writeln!(
                svg,
                r#"<circle class="item" cx="{x:.1}" cy="{y:.1}" r="5"/>"#
            );
        }
        let _ = writeln!(svg, "</g>");
    }
    svg.push_str("</svg>\n");
    svg
}

/// One projected point in screen space.
#[derive(Debug, Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

/// Uniform scale + translation that fits the projected model into the viewport.
#[derive(Debug, Clone, Copy)]
struct Transform {
    scale: f64,
    offset_x: f64,
    offset_y: f64,
}

impl Transform {
    fn apply(self, point: Point) -> Point {
        Point {
            x: point.x * self.scale + self.offset_x,
            y: point.y * self.scale + self.offset_y,
        }
    }
}

const ISO_COS: f64 = 0.866_025_403_784_438_6;
const ISO_SIN: f64 = 0.5;
/// Screen height of an object with `height = 1`.
const HEIGHT_SCALE: f64 = 70.0;

/// Isometric projection of model coordinates `(x, y, z)`, with `z` up.
fn isometric(x: f64, y: f64, z: f64) -> Point {
    Point {
        x: (x - y) * ISO_COS,
        y: (x + y) * ISO_SIN - z,
    }
}

/// Render an isometric 3D preview: every object is a box at its `pos`, with a
/// height taken from the model (see [`object_height`]) and shaded top/side
/// faces. Connections are drawn in 3D under the boxes, and an optional frame
/// moves one marker per connection to suggest flow items.
pub fn to_svg_3d(model: &Model, width: u32, height: u32, frame: Option<Frame>) -> String {
    let width = width.clamp(320, 10_000) as f64;
    let height = height.clamp(240, 10_000) as f64;
    let centers = model_centers(model);
    if model.objects.is_empty() {
        return to_svg_flat(model, width as u32, height as u32, frame);
    }

    // Raw (untransformed) geometry: eight corners per box plus its link anchor.
    struct RawBox {
        name: String,
        kind: ObjectKind,
        bottom: [Point; 4],
        top: [Point; 4],
        anchor: Point,
        depth: f64,
    }
    let mut boxes: Vec<RawBox> = Vec::with_capacity(model.objects.len());
    let mut all_points: Vec<Point> = Vec::new();
    for object in &model.objects {
        let Some((cx, cy)) = centers.get(&object.name).copied() else {
            continue;
        };
        let (footprint_w, footprint_d) = footprint(object.kind);
        let half_w = footprint_w / 2.0;
        let half_d = footprint_d / 2.0;
        let box_height = object_height(model, object) * HEIGHT_SCALE;
        let corners = |z: f64| {
            [
                isometric(cx - half_w, cy - half_d, z),
                isometric(cx + half_w, cy - half_d, z),
                isometric(cx + half_w, cy + half_d, z),
                isometric(cx - half_w, cy + half_d, z),
            ]
        };
        let bottom = corners(0.0);
        let top = corners(box_height);
        let anchor = isometric(cx, cy, box_height * 0.5);
        all_points.extend(bottom);
        all_points.extend(top);
        all_points.push(anchor);
        boxes.push(RawBox {
            name: object.name.clone(),
            kind: object.kind,
            bottom,
            top,
            anchor,
            depth: cx + cy,
        });
    }
    // Labels sit just above the top face; include some room in the fit.
    let label_padding: Vec<Point> = all_points
        .iter()
        .map(|point| Point {
            x: point.x,
            y: point.y - 24.0,
        })
        .collect();
    all_points.extend(label_padding);
    let transform = fit_transform(&all_points, width, height);

    // Draw back rows first so nearer boxes overlap them.
    boxes.sort_by(|left, right| left.depth.total_cmp(&right.depth));

    let mut svg = String::new();
    let _ = writeln!(
        svg,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" data-projection="isometric" aria-label="OpenFlex isometric layout: {}">"#,
        escape(&model.name)
    );
    let _ = writeln!(
        svg,
        r##"<defs>
  <marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
    <path d="M 0 0 L 10 5 L 0 10 z" fill="#334155"/>
  </marker>
  <style>
    .canvas {{ fill: #f1f5f9; }}
    .title {{ font: 600 18px system-ui, sans-serif; fill: #0f172a; }}
    .subtitle {{ font: 12px system-ui, sans-serif; fill: #64748b; }}
    .face-top {{ fill: #e2e8f0; stroke: #334155; stroke-width: 1.2; }}
    .face-left {{ fill: #cbd5e1; stroke: #334155; stroke-width: 1.2; }}
    .face-right {{ fill: #94a3b8; stroke: #334155; stroke-width: 1.2; }}
    .box.source .face-top {{ fill: #dcfce7; }}
    .box.sink .face-top {{ fill: #fee2e2; }}
    .box.processor .face-top {{ fill: #dbeafe; }}
    .link {{ fill: none; stroke: #334155; stroke-width: 1.6; marker-end: url(#arrow); }}
    .name {{ font: 600 12px system-ui, sans-serif; fill: #0f172a; }}
    .kind {{ font: 10px system-ui, sans-serif; fill: #475569; }}
    .item {{ fill: #f59e0b; stroke: #b45309; stroke-width: 1; }}
  </style>
</defs>"##
    );
    let _ = writeln!(
        svg,
        r#"<rect class="canvas" x="0" y="0" width="{width}" height="{height}"/>"#
    );
    let _ = writeln!(
        svg,
        r#"<text class="title" x="{MARGIN}" y="32">OpenFlex isometric layout: {}</text>"#,
        escape(&model.name)
    );
    let _ = writeln!(
        svg,
        r#"<text class="subtitle" x="{MARGIN}" y="50">{} objects, {} connections · isometric projection (x, y, height)</text>"#,
        model.objects.len(),
        model.connections.len()
    );

    let anchors: std::collections::BTreeMap<&str, Point> = boxes
        .iter()
        .map(|boxed| (boxed.name.as_str(), transform.apply(boxed.anchor)))
        .collect();

    let _ = writeln!(svg, r#"<g id="connections">"#);
    for connection in &model.connections {
        let (Some(from), Some(to)) = (
            anchors.get(connection.from.as_str()),
            anchors.get(connection.to.as_str()),
        ) else {
            continue;
        };
        let _ = writeln!(
            svg,
            r#"<line class="link" x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}"/>"#,
            from.x, from.y, to.x, to.y
        );
    }
    let _ = writeln!(svg, "</g>");

    let _ = writeln!(svg, r#"<g id="objects">"#);
    for boxed in &boxes {
        let class = node_class(boxed.kind);
        let bottom: Vec<Point> = boxed
            .bottom
            .iter()
            .map(|point| transform.apply(*point))
            .collect();
        let top: Vec<Point> = boxed
            .top
            .iter()
            .map(|point| transform.apply(*point))
            .collect();
        let polygon = |points: &[Point]| {
            points
                .iter()
                .map(|point| format!("{:.1},{:.1}", point.x, point.y))
                .collect::<Vec<_>>()
                .join(" ")
        };
        let top_face = polygon(&top);
        let left_face = polygon(&[bottom[0], bottom[1], top[1], top[0]]);
        let right_face = polygon(&[bottom[1], bottom[2], top[2], top[1]]);
        let label = transform.apply(Point {
            x: boxed.top.iter().map(|point| point.x).sum::<f64>() / 4.0,
            y: boxed.top.iter().map(|point| point.y).sum::<f64>() / 4.0,
        });
        let _ = writeln!(
            svg,
            r#"<g class="box {class}" data-name="{name}">
  <polygon class="face-left" points="{left_face}"/>
  <polygon class="face-right" points="{right_face}"/>
  <polygon class="face-top" points="{top_face}"/>
  <text class="name" x="{:.1}" y="{:.1}" text-anchor="middle">{name}</text>
  <text class="kind" x="{:.1}" y="{:.1}" text-anchor="middle">{kind}</text>
</g>"#,
            label.x,
            label.y - 4.0,
            label.x,
            label.y + 10.0,
            name = escape(&boxed.name),
            kind = escape(boxed.kind.as_str())
        );
    }
    let _ = writeln!(svg, "</g>");

    if let Some(frame) = frame {
        let progress = frame.progress();
        let _ = writeln!(svg, r#"<g id="items">"#);
        for connection in &model.connections {
            let (Some(from), Some(to)) = (
                centers.get(&connection.from).copied(),
                centers.get(&connection.to).copied(),
            ) else {
                continue;
            };
            let x = from.0 + (to.0 - from.0) * progress;
            let y = from.1 + (to.1 - from.1) * progress;
            let point = transform.apply(isometric(x, y, HEIGHT_SCALE * 0.5));
            let _ = writeln!(
                svg,
                r#"<circle class="item" cx="{:.1}" cy="{:.1}" r="5"/>"#,
                point.x, point.y
            );
        }
        let _ = writeln!(svg, "</g>");
    }
    svg.push_str("</svg>\n");
    svg
}

/// Screen footprint (width, depth) of one object kind in model units.
fn footprint(kind: ObjectKind) -> (f64, f64) {
    match kind {
        ObjectKind::Source | ObjectKind::Sink => (90.0, 90.0),
        ObjectKind::Queue | ObjectKind::Processor => (110.0, 110.0),
        ObjectKind::MultiProcessor => (150.0, 110.0),
        ObjectKind::Combiner | ObjectKind::Separator => (120.0, 110.0),
        ObjectKind::Conveyor => (190.0, 60.0),
        ObjectKind::Rack => (120.0, 80.0),
        ObjectKind::Transporter | ObjectKind::Dispatcher | ObjectKind::BasicTe => (80.0, 80.0),
        _ => (95.0, 95.0),
    }
}

/// Height of an object in the isometric view.
///
/// The model schema does not have a `height` parameter yet, so the value is
/// looked up forward-compatibly in three places: `params.height` (serialised
/// params), `metadata.object_heights[<name>]`, then `metadata.height`. All fall
/// back to 1.
fn object_height(model: &Model, object: &openflex_model::Object) -> f64 {
    if let Ok(serde_json::Value::Object(params)) = serde_json::to_value(&object.params)
        && let Some(height) = params.get("height").and_then(serde_json::Value::as_f64)
    {
        return height.max(0.1);
    }
    if let Some(heights) = model
        .metadata
        .get("object_heights")
        .and_then(|value| value.as_object())
        && let Some(height) = heights
            .get(&object.name)
            .and_then(serde_json::Value::as_f64)
    {
        return height.max(0.1);
    }
    if let Some(height) = model
        .metadata
        .get("height")
        .and_then(serde_json::Value::as_f64)
    {
        return height.max(0.1);
    }
    1.0
}

/// Object centres in model units (grid fallback when no distinct positions).
fn model_centers(model: &Model) -> std::collections::BTreeMap<String, (f64, f64)> {
    let mut centers = std::collections::BTreeMap::new();
    let distinct = {
        let first = model.objects.first().map(|object| object.pos);
        model.objects.iter().any(|object| Some(object.pos) != first)
    };
    if distinct {
        for object in &model.objects {
            centers.insert(object.name.clone(), (object.pos[0], object.pos[1]));
        }
    } else {
        let count = model.objects.len().max(1);
        let columns = (count as f64).sqrt().ceil().max(1.0) as usize;
        for (index, object) in model.objects.iter().enumerate() {
            let column = index % columns;
            let row = index / columns;
            centers.insert(
                object.name.clone(),
                (column as f64 * 220.0, row as f64 * 180.0),
            );
        }
    }
    centers
}

fn fit_transform(points: &[Point], width: f64, height: f64) -> Transform {
    let (min_x, max_x, min_y, max_y) = points.iter().fold(
        (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ),
        |(min_x, max_x, min_y, max_y), point| {
            (
                min_x.min(point.x),
                max_x.max(point.x),
                min_y.min(point.y),
                max_y.max(point.y),
            )
        },
    );
    let span_x = (max_x - min_x).max(1.0);
    let span_y = (max_y - min_y).max(1.0);
    let usable_w = (width - 2.0 * MARGIN).max(1.0);
    let usable_h = (height - 2.0 * MARGIN - 60.0).max(1.0);
    let scale = (usable_w / span_x).min(usable_h / span_y);
    let offset_x = MARGIN + (usable_w - span_x * scale) / 2.0 - min_x * scale;
    let offset_y = MARGIN + 60.0 + (usable_h - span_y * scale) / 2.0 - min_y * scale;
    Transform {
        scale,
        offset_x,
        offset_y,
    }
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
        let svg = to_svg_flat(&model, 800, 600, None);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("Source1"));
        assert!(svg.contains("Queue1"));
        assert!(svg.contains("Sink1"));
        assert!(svg.contains("class=\"link\""));
        assert!(svg.trim_end().ends_with("</svg>"));
    }

    #[test]
    fn isometric_layout_draws_shaded_boxes_and_3d_links() {
        let model = openflex_model::Model::from_json(
            r#"{
              "format": "openflex-model", "version": 1, "name": "iso-demo",
              "objects": [
                { "name": "Source1", "kind": "source", "pos": [0, 0] },
                { "name": "Queue1", "kind": "queue", "pos": [10, 0] },
                { "name": "Sink1", "kind": "sink", "pos": [20, 0] }
              ],
              "connections": [
                { "from": "Source1", "to": "Queue1" },
                { "from": "Queue1", "to": "Sink1" }
              ]
            }"#,
        )
        .expect("fixture model");
        let svg = to_svg_3d(&model, 900, 600, None);
        assert!(svg.contains("data-projection=\"isometric\""));
        assert!(svg.contains("Source1") && svg.contains("Queue1") && svg.contains("Sink1"));
        assert!(svg.contains("class=\"link\""));
        assert!(svg.matches("<polygon").count() >= 9, "three faces per box");
        assert!(
            svg.contains("face-top") && svg.contains("face-left") && svg.contains("face-right")
        );
        assert!(svg.trim_end().ends_with("</svg>"));

        let frame = to_svg_3d(&model, 900, 600, Some(Frame { index: 1, count: 3 }));
        assert!(frame.contains("class=\"item\""));
    }
}
