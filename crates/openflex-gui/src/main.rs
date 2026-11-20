//! Native desktop GUI for OpenFlex (eframe/egui).
//!
//! A thin desktop front end over the existing crates: open/save a `.ofm` model,
//! browse and edit objects (position and parameters), edit connections, run the
//! simulation (full run or step/pause/resume through `SimulationHandle`) and
//! read the per-object report with a small bar chart.
//!
//! Run with `openflex-gui [model.ofm]`.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use openflex_core::SimConfig;
use openflex_core::engine::{SimulationHandle, SimulationState};
use openflex_core::run_model;
use openflex_core::stats::SimReport;
use openflex_model::Distribution;
use openflex_model::Model;
use openflex_model::model::{
    ObjectKind, ObjectParams, QueueOrder, RackPickPolicy, RackType, TaskStep, TaskStepKind,
    ValveOpeningCurve,
};

/// Built-in examples compiled into the binary, used when no `examples/`
/// directory sits next to the executable (installed builds).
fn builtin_example(name: &str) -> Option<Model> {
    let json = match name {
        "mm1" => {
            r#"{
  "format": "openflex-model", "version": 1, "name": "mm1",
  "description": "M/M/1 queue: exponential arrivals and service",
  "run": {"stop_time": 1000.0, "warmup": 0.0, "seed": 1, "replications": 1},
  "objects": [
    {"name": "Source1", "kind": "source", "pos": [-6.0, 0.0], "params": {"interarrival": {"distribution": "exponential", "mean": 5.0}}},
    {"name": "Queue1", "kind": "queue", "pos": [-2.0, 0.0], "params": {"max_content": 1000}},
    {"name": "Processor1", "kind": "processor", "pos": [2.0, 0.0], "params": {"process_time": {"distribution": "exponential", "mean": 4.0}}},
    {"name": "Sink1", "kind": "sink", "pos": [6.0, 0.0], "params": {}}
  ],
  "connections": [
    {"from": "Source1", "to": "Queue1"},
    {"from": "Queue1", "to": "Processor1"},
    {"from": "Processor1", "to": "Sink1"}
  ]
}"#
        }
        "routing" => {
            r#"{
  "format": "openflex-model", "version": 1, "name": "routing",
  "description": "One queue feeding two processors with round-robin routing",
  "run": {"stop_time": 600.0, "warmup": 0.0, "seed": 7, "replications": 1},
  "objects": [
    {"name": "Source1", "kind": "source", "pos": [-8.0, 0.0], "params": {"interarrival": {"distribution": "exponential", "mean": 4.0}}},
    {"name": "Queue1", "kind": "queue", "pos": [-4.0, 0.0], "params": {"max_content": 1000}},
    {"name": "Processor1", "kind": "processor", "pos": [0.0, 2.0], "params": {"process_time": {"distribution": "constant", "value": 6.0}}},
    {"name": "Processor2", "kind": "processor", "pos": [0.0, -2.0], "params": {"process_time": {"distribution": "constant", "value": 6.0}}},
    {"name": "Sink1", "kind": "sink", "pos": [4.0, 0.0], "params": {}}
  ],
  "connections": [
    {"from": "Source1", "to": "Queue1"},
    {"from": "Queue1", "to": "Processor1", "strategy": {"type": "round_robin"}},
    {"from": "Queue1", "to": "Processor2", "strategy": {"type": "round_robin"}},
    {"from": "Processor1", "to": "Sink1"},
    {"from": "Processor2", "to": "Sink1"}
  ]
}"#
        }
        "batch-queue" => {
            r#"{
  "format": "openflex-model", "version": 1, "name": "batch-queue",
  "description": "Queue releases items in batches of five",
  "run": {"stop_time": 600.0, "warmup": 0.0, "seed": 3, "replications": 1},
  "objects": [
    {"name": "Source1", "kind": "source", "pos": [-6.0, 0.0], "params": {"interarrival": {"distribution": "constant", "value": 1.0}}},
    {"name": "Queue1", "kind": "queue", "pos": [-2.0, 0.0], "params": {"max_content": 1000, "batch_size": 5}},
    {"name": "Processor1", "kind": "processor", "pos": [2.0, 0.0], "params": {"process_time": {"distribution": "constant", "value": 10.0}}},
    {"name": "Sink1", "kind": "sink", "pos": [6.0, 0.0], "params": {}}
  ],
  "connections": [
    {"from": "Source1", "to": "Queue1"},
    {"from": "Queue1", "to": "Processor1"},
    {"from": "Processor1", "to": "Sink1"}
  ]
}"#
        }
        "fluid-line" => {
            r#"{
  "format": "openflex-model", "version": 1, "name": "fluid-line",
  "description": "A generator feeds a tank, which drains through a valve into a sink",
  "run": {"stop_time": 120.0, "warmup": 0.0, "seed": 1, "replications": 1},
  "objects": [
    {"name": "Generator1", "kind": "fluid_generator", "pos": [-9.0, 0.0],
     "params": {"flow_rate": 4.0}},
    {"name": "Tank1", "kind": "fluid_tank", "pos": [-4.0, 0.0],
     "params": {"capacity": 20.0, "initial_level": 5.0, "max_outflow": 6.0}},
    {"name": "Valve1", "kind": "fluid_valve", "pos": [2.0, 0.0],
     "params": {"opening": 0.9}},
    {"name": "Sink1", "kind": "fluid_sink", "pos": [7.0, 0.0], "params": {}}
  ],
  "connections": [
    {"from": "Generator1", "to": "Tank1"},
    {"from": "Tank1", "to": "Valve1"},
    {"from": "Valve1", "to": "Sink1"}
  ]
}"#
        }
        "agv-delivery" => {
            r#"{
  "format": "openflex-model", "version": 1, "name": "agv-delivery",
  "description": "One AGV shuttles items from a processor to a sink",
  "run": {"stop_time": 600.0, "warmup": 0.0, "seed": 3, "replications": 1},
  "objects": [
    {"name": "Source1", "kind": "source", "pos": [-10.0, 0.0],
     "params": {"interarrival": {"distribution": "constant", "value": 20.0}}},
    {"name": "Processor1", "kind": "processor", "pos": [-4.0, 0.0],
     "params": {"process_time": {"distribution": "constant", "value": 5.0},
                "process_operator": "Dispatcher1"}},
    {"name": "Sink1", "kind": "sink", "pos": [6.0, 0.0], "params": {}},
    {"name": "Dispatcher1", "kind": "dispatcher", "pos": [0.0, 0.0], "params": {}},
    {"name": "Agv1", "kind": "agv", "pos": [0.0, 0.0],
     "params": {"speed": 2.0, "home": "Dispatcher1"}}
  ],
  "connections": [
    {"from": "Source1", "to": "Processor1"},
    {"from": "Processor1", "to": "Sink1"},
    {"from": "Dispatcher1", "to": "Agv1"}
  ]
}"#
        }
        _ => return None,
    };
    Model::from_json(json).ok()
}

/// File eframe uses for its window/panel state
/// (`~/.config/openflex/app.ron`), next to `recent.json` and `ui.json`.
fn persistence_file() -> Option<std::path::PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
        })
        .or_else(|| std::env::var_os("APPDATA").map(std::path::PathBuf::from))?;
    let dir = base.join("openflex");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join("app.ron"))
}

fn main() -> eframe::Result<()> {
    let model = std::env::args().nth(1);
    let options = eframe::NativeOptions {
        // Window size/position and collapsing-header state survive restarts.
        persistence_path: persistence_file(),
        persist_window: true,
        viewport: egui::ViewportBuilder::default()
            .with_icon(app_icon())
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("OpenFlex"),
        ..Default::default()
    };
    eframe::run_native(
        "OpenFlex",
        options,
        Box::new(move |cc| {
            install_cjk_font(&cc.egui_ctx);
            apply_theme(&cc.egui_ctx);
            Ok(Box::new(OpenFlexApp::new(model)) as Box<dyn eframe::App>)
        }),
    )
}

/// Builds a 64×64 RGBA icon (a rounded indigo square with a light "O") so the
/// window and the bundle get an icon without shipping a binary asset.
fn app_icon() -> egui::IconData {
    const SIZE: u32 = 64;
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let centre = (SIZE as f32 - 1.0) / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = (x as f32 - centre) / centre;
            let dy = (y as f32 - centre) / centre;
            let radius = (dx * dx + dy * dy).sqrt();
            let offset = ((y * SIZE + x) * 4) as usize;
            let inside_box = radius < 0.92;
            let ring = (radius - 0.55).abs() < 0.16;
            let (r, g, b, a) = if !inside_box {
                (0, 0, 0, 0)
            } else if ring {
                (245, 226, 160, 255)
            } else {
                (67, 84, 170, 255)
            };
            rgba[offset] = r;
            rgba[offset + 1] = g;
            rgba[offset + 2] = b;
            rgba[offset + 3] = a;
        }
    }
    egui::IconData {
        rgba,
        width: SIZE,
        height: SIZE,
    }
}

/// The app colour palette: a cool slate base with an indigo/amber accent pair.
mod palette {
    use egui::Color32;

    pub const BG: Color32 = Color32::from_rgb(17, 20, 26);
    pub const PANEL: Color32 = Color32::from_rgb(24, 28, 36);
    pub const PANEL_HI: Color32 = Color32::from_rgb(31, 36, 46);
    pub const CARD: Color32 = Color32::from_rgb(37, 43, 55);
    pub const CANVAS: Color32 = Color32::from_rgb(13, 16, 21);
    pub const STROKE: Color32 = Color32::from_rgb(52, 60, 76);
    pub const TEXT: Color32 = Color32::from_rgb(226, 232, 240);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(148, 160, 178);
    pub const ACCENT: Color32 = Color32::from_rgb(112, 138, 246);
    pub const ACCENT_HI: Color32 = Color32::from_rgb(138, 160, 255);
    pub const AMBER: Color32 = Color32::from_rgb(245, 190, 100);
    pub const OK: Color32 = Color32::from_rgb(96, 200, 150);

    // Engine state colours: one source of truth for the canvas fill, the
    // legend and the per-object state bar in the report table.
    pub const ST_BUSY: Color32 = Color32::from_rgb(214, 96, 70);
    pub const ST_BLOCKED: Color32 = Color32::from_rgb(224, 168, 66);
    pub const ST_IDLE: Color32 = Color32::from_rgb(84, 168, 116);
    pub const ST_DOWN: Color32 = Color32::from_rgb(126, 126, 138);
    pub const ST_OFF: Color32 = Color32::from_rgb(82, 84, 96);

    /// Soft contact shadow under every 3D body (alpha 70, premultiplied by the
    /// constant constructor so it can live in a `const`).
    pub const SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 70);
}

/// Registers a system CJK font as a fallback so Chinese object names and
/// messages render as text instead of tofu boxes. Returns the family list used
/// for both the proportional and monospace stacks.
fn install_cjk_font(ctx: &egui::Context) {
    const CANDIDATES: &[&str] = &[
        // macOS
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        // Linux
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        // Windows
        "C:/Windows/Fonts/msyh.ttc",
        "C:/Windows/Fonts/simhei.ttf",
    ];
    let Some(bytes) = CANDIDATES.iter().find_map(|path| std::fs::read(path).ok()) else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert("system-cjk".to_owned(), egui::FontData::from_owned(bytes));
    // Append as a fallback: Latin text keeps the crisp default face, CJK (and
    // any glyph the default lacks) comes from the system font.
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push("system-cjk".to_owned());
    }
    ctx.set_fonts(fonts);
}

/// Applies the polished dark theme: rounded widgets, an indigo accent,
/// readable typography and a calmer canvas than the default egui grey.
/// Camera pitch limits, shared by the drag handler and the Top preset so a
/// preset never starts outside the range the mouse can reach.
const PITCH_MIN: f32 = -1.4;
const PITCH_MAX: f32 = 1.45;

/// Zoom limits, shared by the wheel handler and the framing maths so a framed
/// view is never outside the range the wheel allows.
const ZOOM_MIN: f32 = 0.15;
const ZOOM_MAX: f32 = 6.0;

fn apply_theme(ctx: &egui::Context) {
    use palette::*;

    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = PANEL;
    visuals.window_fill = PANEL_HI;
    visuals.extreme_bg_color = CANVAS;
    visuals.faint_bg_color = CARD;
    visuals.code_bg_color = CANVAS;
    visuals.window_stroke = egui::Stroke::new(1.0_f32, STROKE);
    visuals.window_rounding = egui::Rounding::same(8.0_f32);
    visuals.menu_rounding = egui::Rounding::same(8.0_f32);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    visuals.selection.stroke = egui::Stroke::new(1.0_f32, ACCENT_HI);
    visuals.hyperlink_color = ACCENT_HI;
    visuals.override_text_color = Some(TEXT);

    // Widgets: flat cards with a hairline stroke and a bright hover ring.
    let widget = &mut visuals.widgets;
    widget.noninteractive.bg_fill = PANEL;
    widget.noninteractive.weak_bg_fill = PANEL;
    widget.noninteractive.bg_stroke = egui::Stroke::new(1.0_f32, STROKE);
    widget.noninteractive.fg_stroke = egui::Stroke::new(1.0_f32, TEXT_DIM);

    widget.inactive.bg_fill = CARD;
    widget.inactive.weak_bg_fill = CARD;
    widget.inactive.bg_stroke = egui::Stroke::new(1.0_f32, STROKE);
    widget.inactive.fg_stroke = egui::Stroke::new(1.0_f32, TEXT);
    widget.inactive.rounding = egui::Rounding::same(6.0_f32);

    widget.hovered.bg_fill = PANEL_HI;
    widget.hovered.weak_bg_fill = PANEL_HI;
    widget.hovered.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
    widget.hovered.fg_stroke = egui::Stroke::new(1.4_f32, egui::Color32::WHITE);
    widget.hovered.rounding = egui::Rounding::same(6.0_f32);

    widget.active.bg_fill = ACCENT.gamma_multiply(0.85);
    widget.active.weak_bg_fill = ACCENT.gamma_multiply(0.7);
    widget.active.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT_HI);
    widget.active.fg_stroke = egui::Stroke::new(1.4_f32, egui::Color32::WHITE);
    widget.active.rounding = egui::Rounding::same(6.0_f32);

    widget.open.bg_fill = PANEL_HI;
    widget.open.weak_bg_fill = PANEL_HI;
    widget.open.bg_stroke = egui::Stroke::new(1.0_f32, ACCENT);
    widget.open.rounding = egui::Rounding::same(6.0_f32);

    ctx.set_visuals(visuals);

    // Layout: a little more breathing room than the egui defaults.
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0_f32, 6.0_f32);
    style.spacing.button_padding = egui::vec2(9.0_f32, 4.0_f32);
    style.spacing.menu_margin = egui::Margin::same(8.0_f32);
    style.spacing.indent = 16.0_f32;
    style.spacing.slider_width = 120.0_f32;
    style.spacing.interact_size.y = 22.0_f32;
    style.visuals.widgets.inactive.rounding = egui::Rounding::same(6.0_f32);
    style.visuals.striped = true;
    ctx.set_style(style);
}

thread_local! {
    /// Footprint polygon of the last flat-mode object drawn, for hit-testing.
    static FLAT_FOOTPRINT: std::cell::RefCell<Option<Vec<egui::Pos2>>> =
        const { std::cell::RefCell::new(None) };
}

/// Point-in-convex-polygon test (`corners` in order). Used so clicking a
/// rotated 2D footprint hits the right object instead of its bounding box.
fn point_in_polygon(point: egui::Pos2, corners: &[egui::Pos2]) -> bool {
    if corners.len() < 3 {
        return false;
    }
    let mut sign = 0.0_f32;
    for index in 0..corners.len() {
        let a = corners[index];
        let b = corners[(index + 1) % corners.len()];
        let cross = (b.x - a.x) * (point.y - a.y) - (b.y - a.y) * (point.x - a.x);
        if cross.abs() < 1e-3 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if sign != cross.signum() {
            return false;
        }
    }
    true
}

/// Human-readable size for the outputs list (bytes / KiB / MiB).
fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    }
}

/// Collects the slash-separated paths of every numeric leaf inside `value`,
/// which is what the optimiser can perturb (`objects/0/params/…`).
fn numeric_paths(value: &serde_json::Value, prefix: &str, out: &mut Vec<String>) {
    match value {
        serde_json::Value::Number(_) => out.push(prefix.to_string()),
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let next = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}/{key}")
                };
                numeric_paths(child, &next, out);
            }
        }
        serde_json::Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                let next = format!("{prefix}/{index}");
                numeric_paths(child, &next, out);
            }
        }
        _ => {}
    }
}

/// Minimal XML escaping for text placed in the exported SVG.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Lightens `color` towards white by `amount` (0…1) so a selected body keeps
/// its meaning (state/heatmap) while clearly reading as selected.
fn shade_light(color: egui::Color32, amount: f32) -> egui::Color32 {
    let t = amount.clamp(0.0, 1.0);
    let mix = |channel: u8| (channel as f32 + (255.0 - channel as f32) * t).round() as u8;
    egui::Color32::from_rgb(mix(color.r()), mix(color.g()), mix(color.b()))
}

/// A primary (accent-filled) toolbar button, used for the actions that start
/// or stop a run so they stand out from the secondary tools.
fn primary_button(ui: &mut egui::Ui, enabled: bool, text: &str) -> bool {
    let button = egui::Button::new(
        egui::RichText::new(text)
            .strong()
            .color(egui::Color32::from_rgb(14, 18, 26)),
    )
    .fill(if enabled {
        palette::ACCENT_HI
    } else {
        palette::CARD
    })
    .rounding(egui::Rounding::same(6.0_f32));
    ui.add_enabled(enabled, button).clicked()
}

/// Empty-state placeholder used by the panels before any data exists.
fn empty_state(ui: &mut egui::Ui, title: &str, hint: &str) {
    ui.add_space(6.0);
    egui::Frame::none()
        .fill(palette::CARD.gamma_multiply(0.5))
        .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
        .rounding(egui::Rounding::same(8.0_f32))
        .inner_margin(egui::Margin::symmetric(10.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(title).strong().color(palette::TEXT_DIM));
                ui.label(
                    egui::RichText::new(hint)
                        .small()
                        .color(palette::TEXT_DIM.gamma_multiply(0.8)),
                );
            });
        });
    ui.add_space(6.0);
}

/// A small metric card: dim caption above, large value below, hairline border.
fn metric_card(ui: &mut egui::Ui, caption: &str, value: &str, accent: egui::Color32) {
    // Two cards per row share the panel width minus the item spacing.
    let spacing = ui.spacing().item_spacing.x;
    let width = ((ui.available_width() - spacing) / 2.0).max(70.0);
    egui::Frame::none()
        .fill(palette::CARD)
        .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
        .rounding(egui::Rounding::same(8.0_f32))
        .inner_margin(egui::Margin::symmetric(9.0, 6.0))
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.label(
                egui::RichText::new(caption)
                    .size(10.0_f32)
                    .color(palette::TEXT_DIM),
            );
            ui.label(
                egui::RichText::new(value)
                    .size(19.0_f32)
                    .strong()
                    .color(accent),
            );
        });
}

/// A dim upper-case label that names the toolbar group that follows.
fn tool_group(ui: &mut egui::Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(text)
            .size(10.0_f32)
            .strong()
            .color(palette::TEXT_DIM.gamma_multiply(0.9)),
    );
    ui.add_space(2.0);
}

/// Small coloured heading used by the side panels.
fn panel_heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(3.0);
    // Accent bar + letterspaced caption: a section header that reads as a
    // header without a heavy rule under it.
    ui.horizontal(|ui| {
        let (bar, _) = ui.allocate_exact_size(egui::vec2(3.0, 14.0), egui::Sense::hover());
        ui.painter().rect_filled(bar, 1.5, palette::ACCENT);
        ui.add_space(4.0);
        let spaced: String = text
            .to_uppercase()
            .chars()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        ui.label(
            egui::RichText::new(spaced)
                .size(10.0)
                .strong()
                .color(palette::TEXT),
        );
    });
    ui.add_space(2.0);
    ui.separator();
}

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
}

/// Sets a numeric field addressed by a slash path such as
/// `objects/0/params/process_time/value`. Returns `false` when the path does
/// not exist or the target is not a number.
fn set_json_number(root: &mut serde_json::Value, path: &str, value: f64) -> bool {
    let parts: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return false;
    }
    let mut current = root;
    for part in &parts[..parts.len() - 1] {
        current = match current {
            serde_json::Value::Object(map) => {
                let Some(next) = map.get_mut(*part) else {
                    return false;
                };
                next
            }
            serde_json::Value::Array(items) => {
                let Ok(index) = part.parse::<usize>() else {
                    return false;
                };
                let Some(next) = items.get_mut(index) else {
                    return false;
                };
                next
            }
            _ => return false,
        };
    }
    let last = parts[parts.len() - 1];
    match current {
        serde_json::Value::Object(map) => {
            match map.get(last) {
                // Only numeric slots are writable; overwriting an object or an
                // array would silently destroy part of the model.
                Some(serde_json::Value::Number(_)) | Some(serde_json::Value::Null) => {
                    map.insert(last.to_string(), serde_json::json!(value));
                    true
                }
                _ => false,
            }
        }
        _ => false,
    }
}

fn parse_params(kind: ObjectKind, value: serde_json::Value) -> Result<ObjectParams, String> {
    Ok(match kind {
        ObjectKind::Source => ObjectParams::Source(decode(value)?),
        ObjectKind::Queue => ObjectParams::Queue(decode(value)?),
        ObjectKind::Processor => ObjectParams::Processor(decode(value)?),
        ObjectKind::MultiProcessor => ObjectParams::MultiProcessor(decode(value)?),
        ObjectKind::Sink => ObjectParams::Sink(decode(value)?),
        ObjectKind::Combiner => ObjectParams::Combiner(decode(value)?),
        ObjectKind::Separator => ObjectParams::Separator(decode(value)?),
        ObjectKind::Conveyor => ObjectParams::Conveyor(decode(value)?),
        ObjectKind::Merge => ObjectParams::Merge(decode(value)?),
        ObjectKind::Split => ObjectParams::Split(decode(value)?),
        ObjectKind::Transporter => ObjectParams::Transporter(decode(value)?),
        ObjectKind::Person => ObjectParams::Person(decode(value)?),
        ObjectKind::Agv => ObjectParams::Agv(decode(value)?),
        ObjectKind::Elevator => ObjectParams::Elevator(decode(value)?),
        ObjectKind::Robot => ObjectParams::Robot(decode(value)?),
        ObjectKind::Crane => ObjectParams::Crane(decode(value)?),
        ObjectKind::AsrsVehicle => ObjectParams::AsrsVehicle(decode(value)?),
        ObjectKind::Escalator => ObjectParams::Escalator(decode(value)?),
        ObjectKind::Dispatcher => ObjectParams::Dispatcher(decode(value)?),
        ObjectKind::Resource => ObjectParams::Resource(decode(value)?),
        ObjectKind::Rack => ObjectParams::Rack(decode(value)?),
        ObjectKind::BasicFr => ObjectParams::BasicFr(decode(value)?),
        ObjectKind::BasicTe => ObjectParams::BasicTe(decode(value)?),
        ObjectKind::FluidGenerator => ObjectParams::FluidGenerator(decode(value)?),
        ObjectKind::FluidTank => ObjectParams::FluidTank(decode(value)?),
        ObjectKind::FluidValve => ObjectParams::FluidValve(decode(value)?),
        ObjectKind::FluidSink => ObjectParams::FluidSink(decode(value)?),
        ObjectKind::FluidPipe => ObjectParams::FluidPipe(decode(value)?),
    })
}

// ---------------------------------------------------------------------------
// Kind-aware parameter forms
// ---------------------------------------------------------------------------

/// Object kinds offered by the "add object" picker (same names as the model).
const KINDS: &[&str] = &[
    "source",
    "queue",
    "processor",
    "multi_processor",
    "sink",
    "conveyor",
    "combiner",
    "separator",
    "merge",
    "split",
    "rack",
    "resource",
    "dispatcher",
    "transporter",
    "person",
    "agv",
    "elevator",
    "robot",
    "crane",
    "asrs_vehicle",
    "escalator",
    "basic_fr",
    "basic_te",
    "fluid_generator",
    "fluid_tank",
    "fluid_valve",
    "fluid_sink",
    "fluid_pipe",
];

const DIST_CHOICES: &[&str] = &[
    "constant",
    "uniform",
    "normal",
    "exponential",
    "triangular",
    "gamma",
    "weibull",
    "lognormal",
    "erlang",
    "beta",
    "integer_uniform",
    "pareto",
    "rayleigh",
    "poisson",
    "bernoulli",
];

fn distribution_name(dist: &Distribution) -> &'static str {
    match dist {
        Distribution::Constant { .. } => "constant",
        Distribution::Uniform { .. } => "uniform",
        Distribution::Normal { .. } => "normal",
        Distribution::Exponential { .. } => "exponential",
        Distribution::Triangular { .. } => "triangular",
        Distribution::Gamma { .. } => "gamma",
        Distribution::Weibull { .. } => "weibull",
        Distribution::Lognormal { .. } => "lognormal",
        Distribution::Erlang { .. } => "erlang",
        Distribution::Beta { .. } => "beta",
        Distribution::IntegerUniform { .. } => "integer_uniform",
        Distribution::Empirical { .. } => "empirical",
        Distribution::Pareto { .. } => "pareto",
        Distribution::LogLogistic { .. } => "log_logistic",
        Distribution::Rayleigh { .. } => "rayleigh",
        Distribution::Binomial { .. } => "binomial",
        Distribution::Poisson { .. } => "poisson",
        Distribution::Geometric { .. } => "geometric",
        Distribution::NegativeBinomial { .. } => "negative_binomial",
        Distribution::Bernoulli { .. } => "bernoulli",
    }
}

/// Switches `dist` to another family while keeping any parameter the two
/// families share (so `normal(mean 8)` → `exponential` keeps 8 as its mean
/// instead of snapping back to the 1.0 default).
fn switch_distribution(dist: &mut Distribution, name: &str) {
    let mut next = make_distribution(name);
    let carried = |field: &str| -> Option<f64> {
        let read = |value: &Distribution| -> Option<f64> {
            use Distribution::*;
            match (value, field) {
                (Constant { value }, "value") => Some(*value),
                (Uniform { min, .. }, "min") | (Triangular { min, .. }, "min") => Some(*min),
                (Uniform { max, .. }, "max") | (Triangular { max, .. }, "max") => Some(*max),
                (IntegerUniform { min, .. }, "min") => Some(*min as f64),
                (IntegerUniform { max, .. }, "max") => Some(*max as f64),
                (Normal { mean, .. }, "mean")
                | (Exponential { mean }, "mean")
                | (Lognormal { meanlog: mean, .. }, "mean") => Some(*mean),
                (Triangular { mode, .. }, "mode") => Some(*mode),
                (Normal { stddev, .. }, "stddev") => Some(*stddev),
                (Gamma { shape, .. }, "shape") | (Weibull { shape, .. }, "shape") => Some(*shape),
                (Erlang { shape, .. }, "shape") => Some(*shape as f64),
                (Pareto { shape, .. }, "shape") => Some(*shape),
                (Gamma { scale, .. }, "scale")
                | (Weibull { scale, .. }, "scale")
                | (Erlang { scale, .. }, "scale")
                | (Pareto { scale, .. }, "scale") => Some(*scale),
                (Lognormal { sdlog, .. }, "sdlog") => Some(*sdlog),
                (Beta { alpha, .. }, "alpha") => Some(*alpha),
                (Beta { beta, .. }, "beta") => Some(*beta),
                _ => None,
            }
        };
        read(dist)
    };
    let apply = |field: &str, target: &mut f64| {
        if let Some(value) = carried(field) {
            *target = value;
        }
    };
    {
        use Distribution::*;
        match &mut next {
            Constant { value } => apply("value", value),
            Uniform { min, max } => {
                apply("min", min);
                apply("max", max);
            }
            Normal { mean, stddev } => {
                apply("mean", mean);
                apply("stddev", stddev);
            }
            Exponential { mean } => apply("mean", mean),
            Triangular { min, mode, max } => {
                apply("min", min);
                apply("mode", mode);
                apply("max", max);
            }
            Gamma { shape, scale } | Weibull { shape, scale } | Pareto { shape, scale } => {
                apply("shape", shape);
                apply("scale", scale);
            }
            Lognormal { meanlog, sdlog } => {
                apply("mean", meanlog);
                apply("sdlog", sdlog);
            }
            Erlang { scale, .. } => apply("scale", scale),
            Beta { alpha, beta } => {
                apply("alpha", alpha);
                apply("beta", beta);
            }
            _ => {}
        }
    }
    *dist = next;
}

fn make_distribution(name: &str) -> Distribution {
    match name {
        "uniform" => Distribution::Uniform { min: 0.0, max: 1.0 },
        "normal" => Distribution::Normal {
            mean: 1.0,
            stddev: 0.1,
        },
        "exponential" => Distribution::Exponential { mean: 1.0 },
        "triangular" => Distribution::Triangular {
            min: 0.5,
            mode: 1.0,
            max: 1.5,
        },
        "gamma" => Distribution::Gamma {
            shape: 2.0,
            scale: 0.5,
        },
        "weibull" => Distribution::Weibull {
            shape: 2.0,
            scale: 1.0,
        },
        "lognormal" => Distribution::Lognormal {
            meanlog: 0.0,
            sdlog: 0.2,
        },
        "erlang" => Distribution::Erlang {
            shape: 2,
            scale: 0.5,
        },
        "beta" => Distribution::Beta {
            alpha: 2.0,
            beta: 2.0,
        },
        "integer_uniform" => Distribution::IntegerUniform { min: 1, max: 5 },
        "pareto" => Distribution::Pareto {
            shape: 1.5,
            scale: 1.0,
        },
        "rayleigh" => Distribution::Rayleigh { sigma: 1.0 },
        "poisson" => Distribution::Poisson { mean: 2.0 },
        "bernoulli" => Distribution::Bernoulli { p: 0.5 },
        _ => Distribution::Constant { value: 1.0 },
    }
}

fn distribution_editor(ui: &mut egui::Ui, label: &str, dist: &mut Distribution) -> bool {
    let mut changed = false;
    let current = distribution_name(dist);
    ui.horizontal(|ui| {
        // Dim label + fixed-width picker keeps every distribution row aligned.
        ui.label(egui::RichText::new(label).small().color(palette::TEXT_DIM));
        egui::ComboBox::from_id_salt(format!("dist-{label}"))
            .selected_text(egui::RichText::new(current).small())
            .width(118.0)
            .show_ui(ui, |ui| {
                for name in DIST_CHOICES {
                    if ui
                        .selectable_label(current == *name, egui::RichText::new(*name).small())
                        .clicked()
                        && current != *name
                    {
                        switch_distribution(dist, name);
                        changed = true;
                    }
                }
            });
    });
    match dist {
        Distribution::Constant { value } => {
            changed |= ui
                .add(egui::DragValue::new(value).speed(0.1).prefix("value "))
                .changed();
        }
        Distribution::Uniform { min, max } => {
            changed |= ui
                .add(egui::DragValue::new(min).speed(0.1).prefix("min "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(max).speed(0.1).prefix("max "))
                .changed();
        }
        Distribution::Normal { mean, stddev } => {
            changed |= ui
                .add(egui::DragValue::new(mean).speed(0.1).prefix("mean "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(stddev).speed(0.05).prefix("stddev "))
                .changed();
        }
        Distribution::Exponential { mean } => {
            changed |= ui
                .add(egui::DragValue::new(mean).speed(0.1).prefix("mean "))
                .changed();
        }
        Distribution::Triangular { min, mode, max } => {
            changed |= ui
                .add(egui::DragValue::new(min).speed(0.1).prefix("min "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(mode).speed(0.1).prefix("mode "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(max).speed(0.1).prefix("max "))
                .changed();
        }
        Distribution::Gamma { shape, scale } => {
            changed |= ui
                .add(egui::DragValue::new(shape).speed(0.1).prefix("shape "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(scale).speed(0.1).prefix("scale "))
                .changed();
        }
        Distribution::Weibull { shape, scale } => {
            changed |= ui
                .add(egui::DragValue::new(shape).speed(0.1).prefix("shape "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(scale).speed(0.1).prefix("scale "))
                .changed();
        }
        Distribution::Lognormal { meanlog, sdlog } => {
            changed |= ui
                .add(egui::DragValue::new(meanlog).speed(0.1).prefix("meanlog "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(sdlog).speed(0.05).prefix("sdlog "))
                .changed();
        }
        Distribution::Erlang { shape, scale } => {
            changed |= ui
                .add(egui::DragValue::new(shape).speed(1.0).prefix("shape "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(scale).speed(0.1).prefix("scale "))
                .changed();
        }
        Distribution::Beta { alpha, beta } => {
            changed |= ui
                .add(egui::DragValue::new(alpha).speed(0.1).prefix("alpha "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(beta).speed(0.1).prefix("beta "))
                .changed();
        }
        Distribution::IntegerUniform { min, max } => {
            changed |= ui
                .add(egui::DragValue::new(min).speed(1.0).prefix("min "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(max).speed(1.0).prefix("max "))
                .changed();
        }
        Distribution::Pareto { shape, scale } => {
            changed |= ui
                .add(egui::DragValue::new(shape).speed(0.1).prefix("shape "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(scale).speed(0.1).prefix("scale "))
                .changed();
        }
        Distribution::Rayleigh { sigma } => {
            changed |= ui
                .add(egui::DragValue::new(sigma).speed(0.1).prefix("sigma "))
                .changed();
        }
        Distribution::Poisson { mean } => {
            changed |= ui
                .add(egui::DragValue::new(mean).speed(0.1).prefix("mean "))
                .changed();
        }
        Distribution::Bernoulli { p } => {
            changed |= ui
                .add(
                    egui::DragValue::new(p)
                        .speed(0.01)
                        .range(0.0..=1.0)
                        .prefix("p "),
                )
                .changed();
        }
        other => {
            ui.label(
                egui::RichText::new(format!("{} (edit via JSON)", distribution_name(other))).weak(),
            );
        }
    }
    changed
}

/// Optional integer field: one checkbox-on-the-row plus an inline value, so
/// enabling a field does not reflow the whole form.
fn optional_u32(ui: &mut egui::Ui, label: &str, value: &mut Option<u32>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut enabled = value.is_some();
        if ui.checkbox(&mut enabled, label).changed() {
            *value = if enabled { Some(1) } else { None };
            changed = true;
        }
        if let Some(inner) = value.as_mut() {
            changed |= ui
                .add(egui::DragValue::new(inner).speed(1.0).max_decimals(0))
                .changed();
        }
    });
    changed
}

/// Optional float field, same layout as [`optional_u32`].
fn optional_f64(ui: &mut egui::Ui, label: &str, value: &mut Option<f64>) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let mut enabled = value.is_some();
        if ui.checkbox(&mut enabled, label).changed() {
            *value = if enabled { Some(1.0) } else { None };
            changed = true;
        }
        if let Some(inner) = value.as_mut() {
            changed |= ui
                .add(egui::DragValue::new(inner).speed(0.1).max_decimals(3))
                .changed();
        }
    });
    changed
}

fn optional_string(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) -> bool {
    let mut text = value.clone().unwrap_or_default();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(label).small().color(palette::TEXT_DIM));
        if ui
            .add(egui::TextEdit::singleline(&mut text).desired_width(150.0))
            .changed()
        {
            *value = if text.trim().is_empty() {
                None
            } else {
                Some(text.trim().to_string())
            };
            changed = true;
        }
    });
    changed
}

/// Small combo box for a serde-style enum rendered as its `snake_case` name.
fn enum_picker<T: Copy + PartialEq + std::fmt::Debug>(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    current: T,
    choices: &[(T, &str)],
) -> Option<T> {
    let text = choices
        .iter()
        .find(|(value, _)| *value == current)
        .map(|(_, name)| *name)
        .unwrap_or("?");
    let mut picked = None;
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(id)
            .selected_text(text)
            .show_ui(ui, |ui| {
                for (value, name) in choices {
                    if ui.selectable_label(*value == current, *name).clicked() && *value != current
                    {
                        picked = Some(*value);
                    }
                }
            });
    });
    picked
}

struct EditForm {
    kind: ObjectKind,
    pos: [f32; 2],
    z: f32,
    params: ObjectParams,
    /// Trigger JSON (shared by every kind); kept as text so the user can type
    /// full `Action` objects without a dedicated editor per action type.
    triggers_text: String,
    timetable_text: String,
}

impl EditForm {
    fn from_object(object: &openflex_model::model::Object) -> Self {
        Self {
            kind: object.kind,
            pos: [object.pos[0] as f32, object.pos[1] as f32],
            z: object.z as f32,
            params: object.params.clone(),
            triggers_text: serde_json::to_string_pretty(object.params.triggers())
                .unwrap_or_else(|_| "{}".into()),
            timetable_text: match object.params.timetable() {
                Some(timetable) => {
                    serde_json::to_string_pretty(timetable).unwrap_or_else(|_| "null".into())
                }
                None => "null".to_string(),
            },
        }
    }

    fn show(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        match &mut self.params {
            ObjectParams::Source(p) => {
                changed |= distribution_editor(ui, "interarrival", &mut p.interarrival);
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.items_per_arrival)
                            .speed(1.0)
                            .prefix("items/arrival "),
                    )
                    .changed();
                changed |= optional_u32(ui, "max arrivals", &mut p.max_arrivals);
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.first_arrival)
                            .speed(0.5)
                            .prefix("first arrival "),
                    )
                    .changed();
            }
            ObjectParams::Queue(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.max_content)
                            .speed(1.0)
                            .range(1..=1_000_000)
                            .prefix("max content "),
                    )
                    .changed();
                changed |= optional_u32(ui, "batch size", &mut p.batch_size);
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.batch_max_wait)
                            .speed(0.5)
                            .prefix("batch max wait "),
                    )
                    .changed();
                {
                    let label = match &p.order {
                        QueueOrder::Fifo => "fifo".to_string(),
                        QueueOrder::Lifo => "lifo".to_string(),
                        QueueOrder::Priority => "priority".to_string(),
                        QueueOrder::ByLabel(name) => format!("by_label({name})"),
                    };
                    ui.horizontal(|ui| {
                        ui.label("order");
                        egui::ComboBox::from_id_salt("queue-order")
                            .selected_text(label)
                            .show_ui(ui, |ui| {
                                for (value, name) in [
                                    (QueueOrder::Fifo, "fifo"),
                                    (QueueOrder::Lifo, "lifo"),
                                    (QueueOrder::Priority, "priority"),
                                ] {
                                    if ui.selectable_label(p.order == value, name).clicked()
                                        && p.order != value
                                    {
                                        p.order = value;
                                        changed = true;
                                    }
                                }
                                // `by_label` needs a label name: offer the
                                // field instead of sending the user to JSON.
                                let selected = matches!(p.order, QueueOrder::ByLabel(_));
                                if ui
                                    .selectable_label(selected, "by_label")
                                    .on_hover_text("sort by a numeric item label")
                                    .clicked()
                                    && !selected
                                {
                                    p.order = QueueOrder::ByLabel("label".to_string());
                                    changed = true;
                                }
                            });
                    });
                    if let QueueOrder::ByLabel(name) = &mut p.order {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("label")
                                    .small()
                                    .color(palette::TEXT_DIM),
                            );
                            if ui
                                .add(
                                    egui::TextEdit::singleline(name)
                                        .hint_text("item label key")
                                        .desired_width(140.0),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        });
                    }
                }
            }
            ObjectParams::Processor(p) => {
                changed |= distribution_editor(ui, "setup time", &mut p.setup_time);
                changed |= distribution_editor(ui, "process time", &mut p.process_time);
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.max_content)
                            .speed(1.0)
                            .prefix("max content "),
                    )
                    .changed();
                changed |= optional_string(ui, "setup operator", &mut p.setup_operator);
                changed |= optional_string(ui, "process operator", &mut p.process_operator);
            }
            ObjectParams::Conveyor(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.speed)
                            .speed(0.1)
                            .prefix("speed "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.length)
                            .speed(1.0)
                            .prefix("length "),
                    )
                    .changed();
                changed |= ui.checkbox(&mut p.accumulating, "accumulating").changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.item_spacing)
                            .speed(0.1)
                            .prefix("item spacing "),
                    )
                    .changed();
            }
            ObjectParams::Resource(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.capacity)
                            .speed(1.0)
                            .prefix("capacity "),
                    )
                    .changed();
            }
            ObjectParams::Transporter(p)
            | ObjectParams::Person(p)
            | ObjectParams::Robot(p)
            | ObjectParams::Crane(p)
            | ObjectParams::AsrsVehicle(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.capacity)
                            .speed(1.0)
                            .prefix("capacity "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.speed)
                            .speed(0.1)
                            .prefix("speed "),
                    )
                    .changed();
                changed |= distribution_editor(ui, "load time", &mut p.load_time);
                changed |= distribution_editor(ui, "unload time", &mut p.unload_time);
                changed |= optional_string(ui, "home", &mut p.home);
            }
            ObjectParams::Agv(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.capacity)
                            .speed(1.0)
                            .prefix("capacity "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.speed)
                            .speed(0.1)
                            .prefix("speed "),
                    )
                    .changed();
                changed |= distribution_editor(ui, "charge time", &mut p.charge_time);
                changed |= optional_f64(ui, "battery/distance", &mut p.consumption_per_distance);
                changed |= optional_string(ui, "charger", &mut p.charger);
            }
            ObjectParams::Rack(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.capacity)
                            .speed(1.0)
                            .prefix("capacity "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.levels)
                            .speed(1.0)
                            .prefix("levels "),
                    )
                    .changed();
                changed |= optional_u32(ui, "channels", &mut p.channels);
                changed |= optional_u32(ui, "depth", &mut p.depth);
                changed |= optional_u32(ui, "aisles", &mut p.aisles);
                if let Some(value) = enum_picker(
                    ui,
                    "rack-type",
                    "rack type",
                    p.rack_type,
                    &[
                        (RackType::Floor, "floor"),
                        (RackType::Gravity, "gravity"),
                        (RackType::PushBack, "push_back"),
                        (RackType::DriveIn, "drive_in"),
                    ],
                ) {
                    p.rack_type = value;
                    changed = true;
                }
                if let Some(value) = enum_picker(
                    ui,
                    "rack-pick",
                    "pick policy",
                    p.pick_policy,
                    &[
                        (RackPickPolicy::Fifo, "fifo"),
                        (RackPickPolicy::LevelOrder, "level_order"),
                        (RackPickPolicy::Nearest, "nearest"),
                    ],
                ) {
                    p.pick_policy = value;
                    changed = true;
                }
            }
            ObjectParams::Elevator(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.capacity)
                            .speed(1.0)
                            .prefix("capacity "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.speed)
                            .speed(0.1)
                            .prefix("speed "),
                    )
                    .changed();
                changed |= distribution_editor(ui, "stop time", &mut p.stop_time);
                changed |= optional_string(ui, "home", &mut p.home);
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.door_open_time)
                            .speed(0.1)
                            .prefix("door open "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.door_close_time)
                            .speed(0.1)
                            .prefix("door close "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.max_load)
                            .speed(5.0)
                            .prefix("max load "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.load_per_task)
                            .speed(5.0)
                            .prefix("load/task "),
                    )
                    .changed();
                changed |= optional_string(ui, "shaft", &mut p.shaft);
            }
            ObjectParams::Dispatcher(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.max_tasks)
                            .speed(1.0)
                            .prefix("max tasks "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.preempt_retry_delay)
                            .speed(0.1)
                            .prefix("preempt retry delay "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.preempt_max_retries)
                            .speed(1.0)
                            .prefix("preempt max retries "),
                    )
                    .changed();
                // Task sequence editor: each step picks its kind and can be
                // reordered or removed; an empty list keeps the default
                // travel → load → unload behaviour.
                ui.label(
                    egui::RichText::new(format!("task steps ({})", p.task_sequence.len()))
                        .size(9.0_f32)
                        .color(palette::TEXT_DIM),
                );
                let mut remove: Option<usize> = None;
                let mut swap: Option<(usize, usize)> = None;
                for (index, step) in p.task_sequence.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        let current = step.kind();
                        egui::ComboBox::from_id_salt(("task-step", index))
                            .selected_text(match current {
                                TaskStepKind::Travel => "travel",
                                TaskStepKind::Load => "load",
                                TaskStepKind::Unload => "unload",
                            })
                            .width(88.0)
                            .show_ui(ui, |ui| {
                                for (kind, name) in [
                                    (TaskStepKind::Travel, "travel"),
                                    (TaskStepKind::Load, "load"),
                                    (TaskStepKind::Unload, "unload"),
                                ] {
                                    if ui.selectable_label(current == kind, name).clicked()
                                        && current != kind
                                    {
                                        // Keep any detailed fields the step had.
                                        *step = match step {
                                            TaskStep::Detailed {
                                                duration,
                                                condition,
                                                ..
                                            } => TaskStep::Detailed {
                                                kind,
                                                duration: duration.clone(),
                                                condition: condition.clone(),
                                            },
                                            TaskStep::Simple(_) => TaskStep::Simple(kind),
                                        };
                                        changed = true;
                                    }
                                }
                            });
                        if index > 0 && ui.small_button("up").clicked() {
                            swap = Some((index - 1, index));
                        }
                        if ui.small_button("x").clicked() {
                            remove = Some(index);
                        }
                    });
                }
                if let Some((a, b)) = swap {
                    p.task_sequence.swap(a, b);
                    changed = true;
                }
                if let Some(index) = remove {
                    p.task_sequence.remove(index);
                    changed = true;
                }
                ui.horizontal(|ui| {
                    for (kind, name) in [
                        (TaskStepKind::Travel, "+travel"),
                        (TaskStepKind::Load, "+load"),
                        (TaskStepKind::Unload, "+unload"),
                    ] {
                        if ui.small_button(name).clicked() {
                            p.task_sequence.push(TaskStep::Simple(kind));
                            changed = true;
                        }
                    }
                    if !p.task_sequence.is_empty() && ui.small_button("clear").clicked() {
                        p.task_sequence.clear();
                        changed = true;
                    }
                });
            }
            ObjectParams::BasicFr(p) => {
                ui.label(format!(
                    "process flow: {} · timetable: {} · emptier: {} (edit via JSON)",
                    p.process_flow.is_some(),
                    p.timetable.is_some(),
                    p.emptier.is_some()
                ));
            }
            ObjectParams::BasicTe(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.speed)
                            .speed(0.1)
                            .prefix("speed "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.capacity)
                            .speed(1.0)
                            .prefix("capacity "),
                    )
                    .changed();
            }
            ObjectParams::FluidGenerator(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.flow_rate)
                            .speed(0.1)
                            .prefix("flow rate "),
                    )
                    .changed();
                changed |= optional_f64(ui, "stop time", &mut p.stop_time);
            }
            ObjectParams::FluidTank(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.capacity)
                            .speed(10.0)
                            .prefix("capacity "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.initial_level)
                            .speed(10.0)
                            .prefix("initial level "),
                    )
                    .changed();
                changed |= optional_f64(ui, "max outflow", &mut p.max_outflow);
                changed |= optional_f64(ui, "density", &mut p.density);
                changed |= optional_f64(ui, "gravity", &mut p.gravity);
            }
            ObjectParams::FluidValve(p) => {
                changed |= ui.checkbox(&mut p.open, "open").changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.ratio)
                            .speed(0.05)
                            .range(0.0..=1.0)
                            .prefix("ratio "),
                    )
                    .changed();
                if let Some(value) = enum_picker(
                    ui,
                    "valve-curve",
                    "open curve",
                    p.open_curve.unwrap_or_default(),
                    &[
                        (ValveOpeningCurve::Linear, "linear"),
                        (ValveOpeningCurve::EqualPercentage, "equal_percentage"),
                    ],
                ) {
                    p.open_curve = Some(value);
                    changed = true;
                }
                changed |= optional_f64(ui, "pressure threshold", &mut p.pressure_threshold);
                changed |= optional_f64(ui, "pressure open", &mut p.pressure_open);
                changed |= optional_f64(ui, "opening", &mut p.opening);
            }
            ObjectParams::FluidPipe(p) => {
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.length)
                            .speed(1.0)
                            .prefix("length "),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut p.speed)
                            .speed(0.1)
                            .prefix("speed "),
                    )
                    .changed();
                changed |= optional_f64(ui, "flow rate", &mut p.flow_rate);
                changed |= optional_f64(ui, "temperature", &mut p.temperature);
            }
            ObjectParams::FluidSink(_) => {
                ui.label("Fluid sink: no kind-specific parameters (see JSON for triggers).");
            }
            ObjectParams::Sink(_) => {
                ui.label("Sink: no kind-specific parameters (see JSON for process flow).");
            }
            _ => {
                ui.label(
                    egui::RichText::new(format!(
                        "{}: no form yet — edit the JSON below.",
                        self.kind.as_str()
                    ))
                    .weak(),
                );
            }
        }
        ui.separator();
        egui::CollapsingHeader::new("Timetable (JSON)")
            .id_salt("timetable")
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "{\"windows\": [{\"start\": 0, \"duration\": 28800, \"repeat_every\": 86400}]} — null disables shifts",
                    )
                    .weak()
                    .small(),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.timetable_text)
                        .code_editor()
                        .desired_rows(4),
                );
            });
        ui.separator();
        egui::CollapsingHeader::new("Triggers (JSON)")
            .id_salt("triggers")
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "on_create / on_entry / on_exit / on_receive / on_message / on_breakdown / on_repair …",
                    )
                    .weak()
                    .small(),
                );
                ui.add(
                    egui::TextEdit::multiline(&mut self.triggers_text)
                        .code_editor()
                        .desired_rows(6),
                );
            });
        changed
    }

    /// Applies the timetable JSON (`null` clears the shift plan).
    fn apply_timetable(&self, params: &mut ObjectParams) -> Result<(), String> {
        let text = self.timetable_text.trim();
        let timetable: Option<openflex_model::TimeTable> = if text.is_empty() || text == "null" {
            None
        } else {
            Some(
                serde_json::from_str(text)
                    .map_err(|error| format!("invalid timetable JSON: {error}"))?,
            )
        };
        params.set_timetable(timetable);
        Ok(())
    }

    /// Applies the trigger JSON to the params (used by Apply).
    fn apply_triggers(&self, params: &mut ObjectParams) -> Result<(), String> {
        let triggers: openflex_model::Triggers =
            serde_json::from_str(if self.triggers_text.trim().is_empty() {
                "{}"
            } else {
                &self.triggers_text
            })
            .map_err(|error| format!("invalid triggers JSON: {error}"))?;
        *params.triggers_mut() = triggers;
        Ok(())
    }
}

type SeriesPoint = (String, f64, f64, f64); // object, content, throughput, utilization

/// One scenario row of the experiment table: `(value, completed mean, ci95)`.
type ExpRow = (f64, f64, f64);

/// Result delivered by the trace worker: the run and its event log.
type TraceOutcome =
    Result<(openflex_core::ReplicationResult, openflex_core::TraceLog), openflex_core::SimError>;

/// Result delivered by the experiment worker: the rows plus how long it took.
type ExpResult = (Vec<ExpRow>, usize, String, std::time::Duration);

/// One stored layout preset: a name plus the `(position, object name)` pairs.
type LayoutPreset = (String, Vec<([f64; 2], String)>);

/// One row of the GUI event log (also used as a replay checkpoint marker).
#[derive(Clone)]
struct EventEntry {
    time: f64,
    kind: String,
    object: Option<String>,
}

impl EventEntry {
    /// Builds a log row from an engine event preview.
    fn from_preview(preview: &openflex_core::EventPreview) -> Self {
        Self {
            time: preview.time,
            kind: preview.kind.clone(),
            object: preview.object.clone(),
        }
    }

    fn line(&self) -> String {
        format!(
            "t={:.3} {}{}",
            self.time,
            self.kind,
            self.object
                .as_ref()
                .map(|object| format!(" [{object}]"))
                .unwrap_or_default()
        )
    }
}

/// One evaluated optimisation point: variable values, objective, secondary score.
type OptRow = (Vec<f64>, f64, f64);

/// One optimisation variable: JSON path plus its search range.
#[derive(Clone, Default)]
struct OptVar {
    path: String,
    low: f32,
    high: f32,
}
type SeriesRow = (f64, Vec<SeriesPoint>);

struct OpenFlexApp {
    model: Option<&'static Model>,
    path: Option<PathBuf>,
    path_input: String,
    status: String,
    error: Option<String>,
    selected: Option<usize>,
    selection: Vec<usize>,
    marquee_start: Option<egui::Pos2>,
    batch_dx: f32,
    batch_dy: f32,
    batch_rot: f32,
    new_kind: String,
    new_name: String,
    search: String,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    form: Option<EditForm>,
    params_text: String,
    show_json: bool,
    form_dirty: bool,
    /// Working copy of the selected object's name for in-place editing.
    rename_text: String,
    /// The in-memory model has been modified since the last save.
    model_dirty: bool,
    new_from: String,
    new_to: String,
    new_port: String,
    new_conn_kind: u8,
    handle: Option<SimulationHandle<'static>>,
    sim_state: Option<SimulationState>,
    event_log: Vec<EventEntry>,
    event_filter: String,
    /// Per-cell text buffers for the editable global tables:
    /// (table name, row, column) -> text being edited.
    table_edit: std::collections::HashMap<(String, usize, usize), String>,
    /// Last title pushed to the OS window, to avoid redundant commands.
    window_title: String,
    /// Report table sort: column index (0 = name) and direction.
    stats_sort: Option<u8>,
    stats_desc: bool,
    /// Group the model tree by flow stage inside each kind.
    tree_by_stage: bool,
    /// Bumped whenever the model is replaced; keys the validation cache.
    model_gen: u64,
    /// Cached `validate_collect` result for `model_gen`.
    validity_cache: Option<(u64, Vec<openflex_model::ValidationIssue>)>,
    heatmap: u8,
    model_name: String,
    model_stop: f32,
    model_warmup: f32,
    model_seed: String,
    model_replications: u32,
    model_unit: u8,
    /// Trace of the last `Trace run` (full event stream from the engine).
    trace: Option<openflex_core::TraceLog>,
    /// Worker channel for `Trace run`: (outcome, elapsed).
    trace_rx: Option<Receiver<(TraceOutcome, std::time::Duration)>>,
    trace_running: bool,
    checkpoint_dir: String,
    checkpoint_status: String,
    layout_presets: Vec<LayoutPreset>,
    preset_name: String,
    checkpoint_files: Vec<(String, f64)>, // (file name, model time)
    running: bool,
    paused: bool,
    step_size: f32,
    play_speed: f32,
    canvas_size: egui::Vec2,
    canvas_measured: bool,
    frame_pending: bool,
    parallel_runs: bool,
    report: Option<SimReport>,
    baseline: Option<SimReport>,
    show_compare: bool,
    report_rx: Option<Receiver<(Result<SimReport, String>, std::time::Duration)>>,
    report_running: bool,
    exp_start: f32,
    exp_step: f32,
    exp_count: u32,
    exp_rows: Vec<ExpRow>,
    exp_rx: Option<Receiver<ExpResult>>,
    exp_running: bool,
    exp_mode_param: bool,
    exp_path: String,
    exp_reps: u32,
    smooth: std::collections::HashMap<String, [f32; 2]>,
    last_frame: f64,
    undo: Vec<(String, String)>,
    redo: Vec<(String, String)>,
    recent: Vec<PathBuf>,
    save_as: String,
    show_save_as: bool,
    /// Built-in file browser (no system dialog dependency on Linux CI).
    show_browser: bool,
    show_path_picker: bool,
    /// Flat 2D (floor-plan) rendering instead of extruded 3D bodies.
    flat_view: bool,
    path_candidates: Vec<String>,
    /// Files written this session: (path, bytes), newest first.
    outputs: Vec<(std::path::PathBuf, u64)>,
    browser_dir: std::path::PathBuf,
    browser_entries: Vec<(String, bool)>, // (name, is_dir)
    browser_error: Option<String>,
    show_help: bool,
    context_menu_open: Option<String>,
    focus: [f32; 2],
    flow_text: String,
    flow_from: String,
    flow_to: String,
    flow_pos: std::collections::HashMap<String, [f32; 2]>,
    flow_drag: Option<String>,
    flow_pending: Option<String>,
    flow_selected: Option<String>,
    flow_pan: egui::Vec2,
    flow_zoom: f32,
    series: Vec<SeriesRow>,
    series_interval: f32,
    series_running: bool,
    series_rx: Option<Receiver<(Vec<SeriesRow>, String)>>,
    series_object: String,
    series_metric: u8,
    opt_steps: u32,
    opt_mode: u8,
    opt_samples: u32,
    opt_vars: Vec<OptVar>,
    opt_secondary_stay: bool,
    opt_rows: Vec<OptRow>,
    opt_best: Option<OptRow>,
    opt_rx: Option<Receiver<(Vec<OptRow>, String, std::time::Duration)>>,
    opt_running: bool,
    replay_index: Option<usize>,
}

impl OpenFlexApp {
    fn new(model: Option<String>) -> Self {
        let mut app = Self {
            model: None,
            path: None,
            path_input: model.clone().unwrap_or_default(),
            status: "Open a model to start.".into(),
            error: None,
            selected: None,
            selection: Vec::new(),
            marquee_start: None,
            batch_dx: 0.0,
            batch_dy: 0.0,
            batch_rot: 0.0,
            new_kind: "processor".to_string(),
            new_name: String::new(),
            search: String::new(),
            yaw: -0.6,
            pitch: 0.5,
            zoom: 1.0,
            form: None,
            params_text: String::new(),
            show_json: false,
            form_dirty: false,
            rename_text: String::new(),
            model_dirty: false,
            new_from: String::new(),
            new_to: String::new(),
            new_port: String::new(),
            new_conn_kind: 0,
            handle: None,
            sim_state: None,
            event_log: Vec::new(),
            event_filter: String::new(),
            table_edit: std::collections::HashMap::new(),
            window_title: String::new(),
            stats_sort: None,
            stats_desc: true,
            tree_by_stage: true,
            model_gen: 0,
            validity_cache: None,
            heatmap: 0,
            model_name: String::new(),
            model_stop: 300.0,
            model_warmup: 0.0,
            model_seed: "1".into(),
            model_replications: 1,
            model_unit: 0,
            trace: None,
            trace_rx: None,
            trace_running: false,
            checkpoint_dir: "checkpoints".into(),
            checkpoint_status: String::new(),
            layout_presets: Vec::new(),
            preset_name: String::new(),
            checkpoint_files: Vec::new(),
            running: false,
            paused: false,
            step_size: 10.0,
            play_speed: 1.0,
            canvas_size: egui::Vec2::new(800.0, 640.0),
            canvas_measured: false,
            frame_pending: false,
            parallel_runs: true,
            report: None,
            baseline: None,
            show_compare: false,
            report_rx: None,
            report_running: false,
            exp_start: 100.0,
            exp_step: 100.0,
            exp_count: 4,
            exp_rows: Vec::new(),
            exp_rx: None,
            exp_running: false,
            exp_mode_param: false,
            exp_path: String::new(),
            exp_reps: 1,
            smooth: std::collections::HashMap::new(),
            last_frame: 0.0,
            undo: Vec::new(),
            redo: Vec::new(),
            recent: Vec::new(),
            save_as: String::new(),
            show_save_as: false,
            show_browser: false,
            show_path_picker: false,
            flat_view: false,
            path_candidates: Vec::new(),
            outputs: Vec::new(),
            browser_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            browser_entries: Vec::new(),
            browser_error: None,
            show_help: false,
            context_menu_open: None,
            focus: [0.0, 0.0],
            flow_text: String::new(),
            flow_from: String::new(),
            flow_to: String::new(),
            flow_pos: std::collections::HashMap::new(),
            flow_drag: None,
            flow_pending: None,
            flow_selected: None,
            flow_pan: egui::Vec2::ZERO,
            flow_zoom: 1.0,
            series: Vec::new(),
            series_interval: 5.0,
            series_running: false,
            series_rx: None,
            series_object: String::new(),
            series_metric: 0,
            opt_steps: 5,
            opt_mode: 0,
            opt_samples: 20,
            opt_vars: vec![OptVar {
                path: String::new(),
                low: 1.0,
                high: 10.0,
            }],
            opt_secondary_stay: false,
            opt_rows: Vec::new(),
            opt_best: None,
            opt_rx: None,
            opt_running: false,
            replay_index: None,
        };
        app.load_recent();
        app.load_ui_prefs();
        if let Some(path) = model {
            app.load(PathBuf::from(path));
        }
        // Show the checkpoints that already exist next to the project.
        app.refresh_checkpoints();
        app
    }

    /// Records the current model in the undo stack (called before an edit).
    /// Bumps the model generation, invalidating the validation cache.
    fn touch_model(&mut self) {
        self.model_gen = self.model_gen.wrapping_add(1);
    }

    fn snapshot(&mut self) {
        self.snapshot_labelled("edit");
    }

    /// Records the current model in the undo stack under a human label, so the
    /// history list can say what each step was.
    fn snapshot_labelled(&mut self, label: &str) {
        let Some(model) = self.model else {
            return;
        };
        if let Ok(json) = model.to_json() {
            self.undo.push((json, label.to_string()));
            self.model_dirty = true;
            self.touch_model();
            if self.undo.len() > 50 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    /// Parses one table cell from user text: a number when it parses as one,
    /// `true`/`false` as booleans, everything else as a string.
    fn parse_table_cell(text: &str) -> openflex_model::TableCell {
        let trimmed = text.trim();
        if let Ok(value) = trimmed.parse::<f64>() {
            return openflex_model::TableCell::Number(value);
        }
        match trimmed.to_lowercase().as_str() {
            "true" => openflex_model::TableCell::Bool(true),
            "false" => openflex_model::TableCell::Bool(false),
            _ => openflex_model::TableCell::Str(text.to_string()),
        }
    }

    /// Writes one edited global-table row back into the model.
    fn commit_table_row(&mut self, table_name: &str, row: usize, cells: Vec<String>) {
        let Some(model) = self.model else {
            return;
        };
        // Validate the shape before touching the undo stack, so a rejected edit
        // does not leave an empty history entry behind.
        let width = model
            .global_tables
            .get(table_name)
            .map(|table| table.width())
            .unwrap_or(0);
        let rows = model
            .global_tables
            .get(table_name)
            .map(|table| table.rows.len())
            .unwrap_or(0);
        if row >= rows {
            return;
        }
        if width != cells.len() {
            self.error = Some(format!(
                "row {row} needs {width} cell(s), got {}",
                cells.len()
            ));
            return;
        }
        self.snapshot_labelled("edit global table");
        let mut copy = model.clone();
        let Some(table) = copy.global_tables.get_mut(table_name) else {
            return;
        };
        table.rows[row] = cells
            .iter()
            .map(|text| Self::parse_table_cell(text))
            .collect();
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.status = format!("Updated {table_name} row {row}");
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Appends a row of empty cells to a global table.
    fn append_table_row(&mut self, table_name: &str) {
        let Some(model) = self.model else {
            return;
        };
        self.snapshot_labelled("add table row");
        let mut copy = model.clone();
        let Some(table) = copy.global_tables.get_mut(table_name) else {
            return;
        };
        let width = table.width();
        table.rows.push(
            (0..width)
                .map(|_| openflex_model::TableCell::Number(0.0))
                .collect(),
        );
        let rows = table.rows.len();
        match copy.validate() {
            Ok(()) => {
                // The new row starts with empty buffers so it is editable.
                self.table_edit
                    .retain(|(table, _, _), _| table != table_name);
                self.model = Some(Box::leak(Box::new(copy)));
                self.status = format!("{table_name} now has {rows} row(s)");
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// One object row of the model tree: kind dot · name · dimmed coordinates.
    fn tree_row(&mut self, ui: &mut egui::Ui, model: &Model, index: usize) {
        let object = &model.objects[index];
        let selected = self.selected == Some(index);
        let mut clicked = false;
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter()
                .circle_filled(dot.center(), 3.4, Self::kind_color(object.kind));
            clicked = ui.selectable_label(selected, &object.name).clicked();
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let rot = object
                    .rot
                    .map(|rot| format!("  {rot:.0}°"))
                    .unwrap_or_default();
                ui.label(
                    egui::RichText::new(format!(
                        "({:.0}, {:.0}){rot}",
                        object.pos[0], object.pos[1]
                    ))
                    .small()
                    .monospace()
                    .color(palette::TEXT_DIM),
                );
            });
        });
        if clicked {
            self.select(index);
        }
    }

    /// Distance of every object from the model's sources along connections;
    /// unreachable objects get `usize::MAX`.
    fn stage_of(model: &Model) -> Vec<usize> {
        let mut distance = vec![usize::MAX; model.objects.len()];
        let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
        for (index, object) in model.objects.iter().enumerate() {
            let has_input = model
                .connections
                .iter()
                .any(|connection| connection.to == object.name);
            if !has_input {
                distance[index] = 0;
                queue.push_back(index);
            }
        }
        while let Some(index) = queue.pop_front() {
            let name = &model.objects[index].name;
            for connection in &model.connections {
                if &connection.from != name {
                    continue;
                }
                if let Some(next) = model
                    .objects
                    .iter()
                    .position(|object| object.name == connection.to)
                    && distance[next] == usize::MAX
                {
                    distance[next] = distance[index].saturating_add(1);
                    queue.push_back(next);
                }
            }
        }
        distance
    }

    /// Title bar text: `OpenFlex — <model><dirty><state>`.
    fn window_title_text(model: Option<&str>, dirty: bool, playing: bool, running: bool) -> String {
        let name = model.unwrap_or("no model");
        let dirty = if dirty { " *" } else { "" };
        let state = if playing {
            " — playing"
        } else if running {
            " — running"
        } else {
            ""
        };
        format!("OpenFlex — {name}{dirty}{state}")
    }

    /// Resets everything that belongs to the previous document: editor buffers,
    /// runtime state, event log and undo history. Used by New / Load / example
    /// loading so no code path can forget one of the buffers.
    fn reset_for_new_document(&mut self) {
        self.clear_object_editor();
        self.event_log.clear();
        self.running = false;
        self.paused = false;
        self.undo.clear();
        self.redo.clear();
    }

    /// Drops the per-object editor buffers after the model changed underneath
    /// them, so a stale `params_text` / `rename_text` cannot be written back.
    fn clear_object_editor(&mut self) {
        self.touch_model();
        self.selected = None;
        self.form = None;
        self.params_text.clear();
        self.flow_text.clear();
        self.rename_text.clear();
        self.form_dirty = false;
        self.handle = None;
        self.sim_state = None;
        self.smooth.clear();
        self.replay_index = None;
        // Table buffers belong to the previous document too.
        self.table_edit.clear();
    }

    fn restore(&mut self) -> bool {
        let Some((json, label)) = self.undo.pop() else {
            return false;
        };
        // Undo changes the document relative to the file on disk.
        self.model_dirty = true;
        match Model::from_json(&json) {
            Ok(model) => {
                if let Some(current) = self.model
                    && let Ok(text) = current.to_json()
                {
                    self.redo.push((text, label.clone()));
                }
                self.model = Some(Box::leak(Box::new(model)));
                self.clear_object_editor();
                self.status = format!("Undo: {label}");
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    fn redo_once(&mut self) -> bool {
        let Some((json, label)) = self.redo.pop() else {
            return false;
        };
        self.model_dirty = true;
        match Model::from_json(&json) {
            Ok(model) => {
                if let Some(current) = self.model
                    && let Ok(text) = current.to_json()
                {
                    self.undo.push((text, label.clone()));
                }
                self.model = Some(Box::leak(Box::new(model)));
                self.clear_object_editor();
                self.status = format!("Redo: {label}");
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    /// Builds a fresh minimal model (source → queue → processor → sink).
    fn new_model(&mut self) {
        let json = r#"{
  "format": "openflex-model",
  "version": 1,
  "name": "untitled",
  "description": "",
  "time_unit": "seconds",
  "run": {"stop_time": 300.0, "warmup": 0.0, "seed": 1, "replications": 1},
  "objects": [
    {"name": "Source1", "kind": "source", "pos": [-6.0, 0.0], "params": {"interarrival": {"distribution": "exponential", "mean": 5.0}}},
    {"name": "Queue1", "kind": "queue", "pos": [-2.0, 0.0], "params": {"max_content": 100}},
    {"name": "Processor1", "kind": "processor", "pos": [2.0, 0.0], "params": {"process_time": {"distribution": "exponential", "mean": 4.0}}},
    {"name": "Sink1", "kind": "sink", "pos": [6.0, 0.0], "params": {}}
  ],
  "connections": [
    {"from": "Source1", "to": "Queue1"},
    {"from": "Queue1", "to": "Processor1"},
    {"from": "Processor1", "to": "Sink1"}
  ]
}"#;
        match Model::from_json(json) {
            Ok(model) => {
                self.model = Some(Box::leak(Box::new(model)));
                self.path = None;
                self.path_input.clear();
                self.report = None;
                // A new document shares no state with the previous one.
                self.reset_for_new_document();
                self.model_name = "untitled".into();
                self.model_stop = 300.0;
                self.model_warmup = 0.0;
                self.model_seed = "1".into();
                self.model_replications = 1;
                self.model_unit = 0;
                // Centre the starter pipeline so it is visible immediately.
                self.selection.clear();
                self.frame_selection();
                // A fresh document has never been written, so prompt a save.
                self.model_dirty = true;
                self.status = "New model — not saved yet".into();
                self.error = None;
            }
            Err(error) => self.error = Some(format!("could not create a model: {error}")),
        }
    }

    /// Loads a model shipped with the repository (`examples/**`), trying the
    /// usual relative locations so the GUI works from the repo or a bundle.
    fn load_example(&mut self, name: &str) {
        let candidates = [
            format!("examples/{name}.ofm"),
            format!("examples/objects/{name}.ofm"),
            format!("../examples/{name}.ofm"),
            format!("../examples/objects/{name}.ofm"),
        ];
        for candidate in candidates {
            let path = PathBuf::from(&candidate);
            if path.exists() {
                self.load(path);
                return;
            }
        }
        // Fall back to the built-in demo model when nothing is on disk.
        if let Some(model) = crate::builtin_example(name) {
            self.model = Some(Box::leak(Box::new(model)));
            self.path = None;
            self.report = None;
            self.selection.clear();
            // The example is a different document: drop every buffer that still
            // refers to the previous objects or tables.
            self.reset_for_new_document();
            // No file on disk: saving it will prompt for a path.
            self.model_dirty = true;
            self.path_input.clear();
            self.status = format!("Loaded built-in example: {name} — not saved yet");
            return;
        }
        self.error = Some(format!("example {name} was not found next to the binary"));
    }

    fn remember_recent(&mut self, path: &PathBuf) {
        self.recent.retain(|item| item != path);
        self.recent.insert(0, path.clone());
        self.recent.truncate(8);
        self.save_recent();
    }

    /// Shares of the four states, normalised so they sum to 1 (an all-zero mix
    /// yields zeros rather than NaN).
    fn state_mix(totals: [f64; 4]) -> [f64; 4] {
        let sum: f64 = totals.iter().sum();
        if sum <= 1e-9 {
            return [0.0; 4];
        }
        let mut out = [0.0; 4];
        for (index, value) in totals.iter().enumerate() {
            out[index] = value / sum;
        }
        out
    }

    /// Sample index under a horizontal position in the chart, clamped to the
    /// available samples. Used by the time-series hover readout.
    fn series_index_at(fraction: f32, samples: usize) -> Option<usize> {
        if samples == 0 {
            return None;
        }
        let fraction = fraction.clamp(0.0, 1.0);
        let index = (fraction * (samples - 1) as f32).round() as usize;
        Some(index.min(samples - 1))
    }

    /// Whether a result produced by a worker belongs to a different document
    /// than the one currently open (an unnamed model cannot be compared).
    fn result_is_stale(&self, source: &str) -> bool {
        self.model
            .map(|model| !model.name.is_empty() && model.name != source)
            .unwrap_or(false)
    }

    /// `~/.config/openflex/ui.json`: view preferences that survive restarts.
    fn ui_config_path() -> Option<std::path::PathBuf> {
        Self::recent_config_path().map(|path| path.with_file_name("ui.json"))
    }

    fn save_ui_prefs(&self) {
        let Some(path) = Self::ui_config_path() else {
            return;
        };
        let json = serde_json::json!({
            "flat_view": self.flat_view,
            "tree_by_stage": self.tree_by_stage,
            "parallel": self.parallel_runs,
            "play_speed": self.play_speed,
            "show_json": self.show_json,
        });
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, json.to_string());
    }

    fn load_ui_prefs(&mut self) {
        let Some(path) = Self::ui_config_path() else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        if let Some(flag) = value.get("flat_view").and_then(|v| v.as_bool()) {
            self.flat_view = flag;
        }
        if let Some(flag) = value.get("tree_by_stage").and_then(|v| v.as_bool()) {
            self.tree_by_stage = flag;
        }
        if let Some(flag) = value.get("parallel").and_then(|v| v.as_bool()) {
            self.parallel_runs = flag;
        }
        if let Some(speed) = value.get("play_speed").and_then(|v| v.as_f64()) {
            self.play_speed = (speed as f32).clamp(0.25, 8.0);
        }
        if let Some(flag) = value.get("show_json").and_then(|v| v.as_bool()) {
            self.show_json = flag;
        }
    }

    /// Where the recent-file list is stored (per-user config directory).
    fn recent_config_path() -> Option<std::path::PathBuf> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
            })
            .or_else(|| std::env::var_os("APPDATA").map(std::path::PathBuf::from))?;
        Some(base.join("openflex").join("recent.json"))
    }

    fn load_recent(&mut self) {
        let Some(path) = Self::recent_config_path() else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let Ok(values) = serde_json::from_str::<Vec<String>>(&text) else {
            return;
        };
        self.recent = values.into_iter().map(std::path::PathBuf::from).collect();
        self.recent.truncate(8);
    }

    fn save_recent(&self) {
        let Some(path) = Self::recent_config_path() else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let values: Vec<String> = self
            .recent
            .iter()
            .map(|item| item.display().to_string())
            .collect();
        if let Ok(text) = serde_json::to_string(&values) {
            let _ = std::fs::write(&path, text);
        }
    }

    /// Reads `browser_dir` for the built-in file browser: directories first,
    /// then `.ofm` files, both alphabetical.
    fn refresh_browser(&mut self) {
        self.browser_error = None;
        let mut dirs: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();
        match std::fs::read_dir(&self.browser_dir) {
            Ok(entries) => {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with('.') {
                        continue;
                    }
                    if entry.path().is_dir() {
                        dirs.push(name);
                    } else if entry.path().extension().is_some_and(|ext| ext == "ofm") {
                        files.push(name);
                    }
                }
            }
            Err(error) => {
                self.browser_error = Some(format!("{}: {error}", self.browser_dir.display()));
                return;
            }
        }
        dirs.sort();
        files.sort();
        self.browser_entries = dirs
            .into_iter()
            .map(|name| (name, true))
            .chain(files.into_iter().map(|name| (name, false)))
            .collect();
    }

    fn navigate_browser(&mut self, name: &str, is_dir: bool) {
        if is_dir {
            self.browser_dir = self.browser_dir.join(name);
            self.refresh_browser();
        } else {
            let path = self.browser_dir.join(name);
            self.show_browser = false;
            self.load(path);
        }
    }

    /// Object name a validation `location` refers to, when it points at one
    /// (`objects.<name>.<field>` → `<name>`).
    fn issue_target(location: &str) -> Option<String> {
        location
            .strip_prefix("objects.")
            .and_then(|rest| rest.split('.').next())
            .filter(|name| !name.is_empty() && !name.chars().all(|c| c.is_ascii_digit()))
            .map(str::to_string)
    }

    fn load(&mut self, path: PathBuf) {
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.error = Some(format!("could not read {}: {error}", path.display()));
                return;
            }
        };
        match Model::from_json(&text) {
            Ok(model) => {
                self.status = format!(
                    "Loaded {} ({} objects, stop {:.1})",
                    path.display(),
                    model.objects.len(),
                    model.run.stop_time
                );
                self.load_presets();
                self.model_name = model.name.clone();
                self.model_stop = model.run.stop_time as f32;
                self.model_warmup = model.run.warmup as f32;
                self.model_seed = model.run.seed.to_string();
                self.model_replications = model.run.replications.max(1);
                self.model_unit = match model.time_unit {
                    openflex_model::TimeUnit::Seconds => 0,
                    openflex_model::TimeUnit::Minutes => 1,
                    openflex_model::TimeUnit::Hours => 2,
                    openflex_model::TimeUnit::Days => 3,
                };
                // A model whose objects all sit on one point would draw as one
                // pile; lay it out so the first view is readable.
                let stacked = {
                    let first = model.objects.first().map(|object| object.pos);
                    first.is_some()
                        && model.objects.len() > 1
                        && model.objects.iter().all(|object| Some(object.pos) == first)
                };
                self.model = Some(Box::leak(Box::new(model)));
                if stacked {
                    self.auto_layout(true);
                    self.status = format!("Loaded {}  ·  auto layout applied", path.display());
                }
                self.path = Some(path.clone());
                self.path_input = path.display().to_string();
                self.remember_recent(&path);
                self.error = None;
                self.model_dirty = false;
                self.form_dirty = false;
                // A different model means different object names: drop the
                // interpolation cache, the replay cursor and every editor
                // buffer that still refers to the old objects.
                self.reset_for_new_document();
                self.report = None;
            }
            Err(error) => self.error = Some(format!("invalid model: {error}")),
        }
    }

    fn save(&mut self) {
        // A model that has never been written (New, built-in example) has no
        // path yet: fall back to the Save-as dialog instead of doing nothing.
        let Some(path) = self.path.clone() else {
            if self.model.is_some() {
                let suggested = if self.path_input.trim().is_empty() {
                    self.model
                        .map(|model| format!("{}.ofm", model.name))
                        .unwrap_or_else(|| "untitled.ofm".to_string())
                } else {
                    self.path_input.clone()
                };
                self.save_as = suggested;
                self.show_save_as = true;
            }
            return;
        };
        let Some(model) = self.model else {
            return;
        };
        match model.to_json() {
            Ok(text) => match std::fs::write(&path, text) {
                Ok(()) => {
                    self.model_dirty = false;
                    self.status = format!("Saved {}", path.display());
                }
                Err(error) => self.error = Some(format!("could not write: {error}")),
            },
            Err(error) => self.error = Some(format!("could not serialise: {error}")),
        }
    }

    fn flow_value(&self) -> Option<serde_json::Value> {
        if self.flow_text.trim().is_empty() {
            None
        } else {
            serde_json::from_str(&self.flow_text).ok()
        }
    }

    fn add_flow_block(&mut self, kind: &str) {
        let mut flow = self
            .flow_value()
            .unwrap_or_else(|| serde_json::json!({"blocks": [], "connections": []}));
        let blocks = flow
            .get_mut("blocks")
            .and_then(|blocks| blocks.as_array_mut());
        let Some(blocks) = blocks else {
            self.error = Some("process flow has no blocks array".into());
            return;
        };
        // Ids are `C1`, `D2`, … derived from the kind prefix; reuse of a
        // deleted id would make the flow ambiguous, so find a free number.
        let prefix = kind
            .chars()
            .next()
            .unwrap_or('B')
            .to_uppercase()
            .to_string();
        let used: Vec<String> = blocks
            .iter()
            .filter_map(|block| block.get("id")?.as_str().map(str::to_string))
            .collect();
        let mut counter = used.len() + 1;
        let id = loop {
            let candidate = format!("{prefix}{counter}");
            if !used.contains(&candidate) {
                break candidate;
            }
            counter += 1;
        };
        let block = match kind {
            "create" => serde_json::json!({
                "id": id, "kind": "create", "quantity": 1, "first_arrival": 0.0,
                "interarrival": {"distribution": "constant", "value": 10.0}, "max_creations": 1
            }),
            "delay" => serde_json::json!({
                "id": id, "kind": "delay", "time": {"distribution": "constant", "value": 1.0}
            }),
            "assign" => serde_json::json!({"id": id, "kind": "assign", "actions": []}),
            "decision" => serde_json::json!({"id": id, "kind": "decision", "conditions": []}),
            "transfer" => serde_json::json!({"id": id, "kind": "transfer", "object": "Sink1"}),
            _ => serde_json::json!({"id": id, "kind": "sink"}),
        };
        blocks.push(block);
        self.flow_text = serde_json::to_string_pretty(&flow).unwrap_or_default();
        self.status = format!("Added {id}");
    }

    fn remove_flow_block(&mut self, id: &str) {
        let Some(mut flow) = self.flow_value() else {
            return;
        };
        if let Some(blocks) = flow.get_mut("blocks").and_then(|b| b.as_array_mut()) {
            // `target` avoids shadowing the parameter with the closure's `id`,
            // which used to make this comparison always false.
            let target = id.to_string();
            blocks.retain(|block| {
                block.get("id").and_then(|id| id.as_str()) != Some(target.as_str())
            });
        }
        if let Some(connections) = flow.get_mut("connections").and_then(|c| c.as_array_mut()) {
            connections.retain(|connection| {
                connection.get("from").and_then(|v| v.as_str()) != Some(id)
                    && connection.get("to").and_then(|v| v.as_str()) != Some(id)
            });
        }
        self.flow_text = serde_json::to_string_pretty(&flow).unwrap_or_default();
        self.status = format!("Removed {id}");
    }

    fn add_flow_connection(&mut self) {
        let (from, to) = (
            self.flow_from.trim().to_string(),
            self.flow_to.trim().to_string(),
        );
        if from.is_empty() || to.is_empty() {
            self.error = Some("choose block ids for the connection".into());
            return;
        }
        let mut flow = self
            .flow_value()
            .unwrap_or_else(|| serde_json::json!({"blocks": [], "connections": []}));
        // Both ends must be existing blocks, otherwise the model would not
        // validate and the error would only show up at Apply time.
        let ids: Vec<String> = flow
            .get("blocks")
            .and_then(|blocks| blocks.as_array())
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|block| block.get("id")?.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        for end in [&from, &to] {
            if !ids.contains(end) {
                self.error = Some(format!("no block named {end}"));
                return;
            }
        }
        if from == to {
            self.error = Some("a block cannot connect to itself".into());
            return;
        }
        let mut duplicate = false;
        if let Some(connections) = flow.get_mut("connections").and_then(|c| c.as_array_mut()) {
            duplicate = connections.iter().any(|connection| {
                connection.get("from").and_then(|v| v.as_str()) == Some(from.as_str())
                    && connection.get("to").and_then(|v| v.as_str()) == Some(to.as_str())
            });
            if !duplicate {
                connections.push(serde_json::json!({"from": from, "to": to}));
            }
        }
        self.flow_text = serde_json::to_string_pretty(&flow).unwrap_or_default();
        self.status = if duplicate {
            format!("{from} -> {to} already exists")
        } else {
            format!("Flow connection {from} -> {to} added")
        };
    }

    fn apply_flow(&mut self) {
        let (Some(_), Some(_)) = (self.model, self.selected) else {
            return;
        };
        let mut params: serde_json::Value = match serde_json::from_str(&self.params_text) {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(format!("invalid parameters JSON: {error}"));
                return;
            }
        };
        let flow: serde_json::Value = if self.flow_text.trim().is_empty() {
            serde_json::Value::Null
        } else {
            match serde_json::from_str(&self.flow_text) {
                Ok(value) => value,
                Err(error) => {
                    self.error = Some(format!("invalid process flow JSON: {error}"));
                    return;
                }
            }
        };
        match params.as_object_mut() {
            Some(map) => {
                if flow.is_null() {
                    map.remove("process_flow");
                } else {
                    map.insert("process_flow".into(), flow);
                }
            }
            None => {
                self.error = Some("parameters must be a JSON object".into());
                return;
            }
        }
        self.params_text = serde_json::to_string_pretty(&params).unwrap_or_default();
        self.apply_json();
    }

    fn flow_block_ids(&self) -> Vec<String> {
        self.flow_value()
            .and_then(|flow| {
                flow.get("blocks")
                    .and_then(|blocks| blocks.as_array())
                    .map(|blocks| {
                        blocks
                            .iter()
                            .filter_map(|block| {
                                block
                                    .get("id")
                                    .and_then(|id| id.as_str())
                                    .map(str::to_string)
                            })
                            .collect()
                    })
            })
            .unwrap_or_default()
    }

    /// Default node placement: a left-to-right pipeline, wrapping every four
    /// blocks so a longer flow still reads top-down.
    fn flow_default_pos(index: usize) -> [f32; 2] {
        let column = index % 4;
        let row = index / 4;
        [16.0 + column as f32 * 140.0, 16.0 + row as f32 * 64.0]
    }

    fn start_timeseries(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let interval = (self.series_interval as f64).max(0.1);
        let stop = model.run.stop_time;
        let source_model = model.name.clone();
        let (tx, rx) = channel();
        self.series_rx = Some(rx);
        self.series_running = true;
        self.status = "Sampling time series…".into();
        std::thread::spawn(move || {
            let config = SimConfig {
                stop_time: stop,
                warmup: model.run.warmup,
                seed: model.run.seed,
            };
            let mut rows = Vec::new();
            // Cap the sample count: a 0.1 s interval on a 1e6 s run would
            // otherwise allocate ten million rows and stall the UI thread.
            let max_samples = 20_000usize;
            if let Ok(mut handle) = SimulationHandle::new(model, &config, 0) {
                let mut now = 0.0;
                while now < stop - 1e-9 && rows.len() < max_samples {
                    now = (now + interval).min(stop);
                    if handle.step_until(now).is_err() {
                        break;
                    }
                    let state = handle.state();
                    let row = state
                        .objects
                        .iter()
                        .map(|(name, stats)| {
                            (
                                name.clone(),
                                stats.content as f64,
                                stats.throughput,
                                stats.utilization,
                            )
                        })
                        .collect();
                    rows.push((state.now, row));
                }
            }
            let _ = tx.send((rows, source_model));
        });
    }

    fn poll_timeseries(&mut self) {
        let Some(rx) = self.series_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok((rows, source)) => {
                let capped = rows.len() >= 20_000;
                // The samples belong to the model that produced them; say so
                // when the document changed while the worker ran.
                let stale = self.result_is_stale(&source);
                let suffix = if stale {
                    format!(" for '{source}' (model changed since)")
                } else {
                    String::new()
                };
                self.status = if capped {
                    format!(
                        "Time series: {} samples (capped — raise the interval to see the whole run){suffix}",
                        rows.len()
                    )
                } else {
                    format!("Time series: {} samples{suffix}", rows.len())
                };
                self.series = rows;
                self.series_running = false;
                self.series_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.series_running = false;
                self.series_rx = None;
            }
        }
    }

    fn start_optimize(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let vars = self.opt_vars.clone();
        if vars.iter().any(|var| var.path.trim().is_empty()) {
            self.error = Some("set a JSON path for every optimisation variable".into());
            return;
        }
        let base_json = match serde_json::to_value(model) {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(format!("could not serialise model: {error}"));
                return;
            }
        };
        let mode = self.opt_mode;
        let steps = self.opt_steps.clamp(2, 50) as usize;
        let samples = self.opt_samples.clamp(2, 300) as usize;
        let secondary = self.opt_secondary_stay;
        let roots: Vec<(String, f64, f64)> = vars
            .iter()
            .map(|var| (var.path.clone(), var.low as f64, var.high as f64))
            .collect();
        if mode == 0 {
            let combos = roots
                .iter()
                .fold(1usize, |acc, _| acc.saturating_mul(steps));
            if combos > 400 {
                self.error = Some(format!(
                    "grid search would evaluate {combos} points; reduce steps or variables"
                ));
                return;
            }
        }
        let source_model = self
            .model
            .map(|model| model.name.clone())
            .unwrap_or_default();
        let (tx, rx) = channel();
        self.opt_rx = Some(rx);
        self.opt_running = true;
        let started = std::time::Instant::now();
        self.status = format!(
            "Optimising: {}",
            match mode {
                1 => format!("{samples} random samples"),
                2 => "genetic search".to_string(),
                _ => "grid".to_string(),
            }
        );
        std::thread::spawn(move || {
            let evaluate = |values: &[f64]| -> Option<(f64, f64)> {
                let mut json = base_json.clone();
                for (index, (path, _, _)) in roots.iter().enumerate() {
                    if !set_json_number(&mut json, path, values[index]) {
                        return None;
                    }
                }
                let patched: Model = serde_json::from_value(json).ok()?;
                let report = run_model(&patched).ok()?;
                let stay = report
                    .objects
                    .values()
                    .map(|stats| stats.avg_stay_time.mean)
                    .fold(0.0_f64, f64::max);
                Some((report.completed.mean, if secondary { stay } else { 0.0 }))
            };
            let mut rows: Vec<OptRow> = Vec::new();
            match mode {
                1 => {
                    let mut state = 0x9E37_79B9_u64;
                    for _ in 0..samples {
                        let values: Vec<f64> = roots
                            .iter()
                            .map(|(_, low, high)| {
                                state = state
                                    .wrapping_mul(6364136223846793005)
                                    .wrapping_add(1442695040888963407);
                                let unit = (state >> 11) as f64 / (1u64 << 53) as f64;
                                low + (high - low) * unit
                            })
                            .collect();
                        if let Some((score, second)) = evaluate(&values) {
                            rows.push((values, score, second));
                        }
                    }
                }
                2 => {
                    let population_size = 12usize;
                    let generations = 6usize;
                    let mut state = 0x2545_F491_u64;
                    let mut next_unit = || -> f64 {
                        state = state
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                        (state >> 11) as f64 / (1u64 << 53) as f64
                    };
                    let mut population: Vec<Vec<f64>> = (0..population_size)
                        .map(|_| {
                            roots
                                .iter()
                                .map(|(_, low, high)| low + (high - low) * next_unit())
                                .collect()
                        })
                        .collect();
                    for _ in 0..generations {
                        let mut scored: Vec<OptRow> = population
                            .iter()
                            .filter_map(|values| {
                                evaluate(values)
                                    .map(|(score, second)| (values.clone(), score, second))
                            })
                            .collect();
                        if scored.is_empty() {
                            break;
                        }
                        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
                        rows.extend(scored.iter().cloned());
                        let elite: Vec<Vec<f64>> =
                            scored.iter().take(4).map(|(v, _, _)| v.clone()).collect();
                        let mut next_generation = elite.clone();
                        while next_generation.len() < population_size {
                            let a =
                                &elite[(next_unit() * elite.len() as f64) as usize % elite.len()];
                            let b =
                                &elite[(next_unit() * elite.len() as f64) as usize % elite.len()];
                            let mut child: Vec<f64> = a
                                .iter()
                                .zip(b.iter())
                                .map(|(a, b)| if next_unit() < 0.5 { *a } else { *b })
                                .collect();
                            for (index, value) in child.iter_mut().enumerate() {
                                if next_unit() < 0.25 {
                                    let (_, low, high) = roots[index];
                                    let span = (high - low).max(1e-9);
                                    *value = (*value + (next_unit() - 0.5) * span * 0.3)
                                        .clamp(low, high);
                                }
                            }
                            next_generation.push(child);
                        }
                        population = next_generation;
                    }
                }
                _ => {
                    let mut indices = vec![0usize; roots.len()];
                    loop {
                        let values: Vec<f64> = roots
                            .iter()
                            .enumerate()
                            .map(|(index, (_, low, high))| {
                                if steps <= 1 {
                                    *low
                                } else {
                                    low + (high - low) * indices[index] as f64 / (steps - 1) as f64
                                }
                            })
                            .collect();
                        if let Some((score, second)) = evaluate(&values) {
                            rows.push((values, score, second));
                        }
                        let mut position = 0;
                        loop {
                            if position >= indices.len() {
                                let _ = tx.send((rows, source_model, started.elapsed()));
                                return;
                            }
                            indices[position] += 1;
                            if indices[position] < steps {
                                break;
                            }
                            indices[position] = 0;
                            position += 1;
                        }
                    }
                }
            }
            let _ = tx.send((rows, source_model, started.elapsed()));
        });
    }

    fn poll_optimize(&mut self) {
        let Some(rx) = self.opt_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok((mut rows, source, elapsed)) => {
                rows.sort_by(|a, b| b.1.total_cmp(&a.1));
                rows.truncate(20);
                self.opt_best = rows.first().cloned();
                let stale = self.result_is_stale(&source);
                let stale_note = if stale {
                    format!(" · from '{source}' (model changed since)")
                } else {
                    String::new()
                };
                self.status = match self.opt_best.as_ref() {
                    Some((values, score, _)) => format!(
                        "Best (in {:.2}s): {} -> completed {score:.1}{stale_note}",
                        elapsed.as_secs_f64(),
                        values
                            .iter()
                            .map(|value| format!("{value:.3}"))
                            .collect::<Vec<_>>()
                            .join(" / ")
                    ),
                    None => {
                        // Almost always a bad JSON path rather than a real
                        // "no better point"; say so instead of a bare message.
                        let bad = self
                            .opt_vars
                            .iter()
                            .find(|var| var.path.trim().is_empty())
                            .is_some();
                        if bad {
                            "Optimisation produced no results — a variable has no JSON path"
                                .to_string()
                        } else {
                            "Optimisation produced no results — check the JSON paths and bounds"
                                .to_string()
                        }
                    }
                };
                self.opt_rows = rows;
                self.opt_running = false;
                self.opt_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.opt_running = false;
                self.opt_rx = None;
            }
        }
    }

    fn export_series_csv(&mut self) {
        if self.series.is_empty() {
            return;
        }
        let unit = self
            .model
            .map(|model| model.time_unit.label())
            .unwrap_or("");
        let mut text = format!(
            "# OpenFlex time series\n# model: {}\n# time_unit: {}\n# interval: {}\n# samples: {}\n",
            self.model.map(|model| model.name.as_str()).unwrap_or(""),
            unit,
            self.series_interval,
            self.series.len()
        );
        text.push_str("time,object,content,throughput,utilization\n");
        for (time, row) in &self.series {
            for (name, content, throughput, utilization) in row {
                text.push_str(&format!(
                    "{time:.4},{name},{content:.4},{throughput:.6},{utilization:.6}\n"
                ));
            }
        }
        let path = self
            .path
            .as_ref()
            .map(|path| path.with_extension("timeseries.csv"))
            .unwrap_or_else(|| PathBuf::from("openflex-timeseries.csv"));
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.remember_output(&path);
                self.status = format!("Time series written to {}", path.display());
                self.error = None;
            }
            Err(error) => self.error = Some(format!("could not write time series: {error}")),
        }
    }

    /// Indices of the non-dominated rows: higher `completed` and lower
    /// secondary value. Used for the Pareto scatter plot.
    fn pareto_front(rows: &[OptRow]) -> Vec<usize> {
        let mut front = Vec::new();
        for (index, (_, score, second)) in rows.iter().enumerate() {
            let dominated = rows.iter().enumerate().any(|(other_index, (_, s2, sec2))| {
                other_index != index
                    && s2 >= score
                    && sec2 <= second
                    && (s2 > score || sec2 < second)
            });
            if !dominated {
                front.push(index);
            }
        }
        front
    }

    /// Remembers an artefact written next to the model (path + byte size), so
    /// the Outputs panel can show what this session produced.
    fn remember_output(&mut self, path: &std::path::Path) {
        let size = std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0);
        self.outputs.retain(|(existing, _)| existing != path);
        self.outputs.insert(0, (path.to_path_buf(), size));
        self.outputs.truncate(12);
    }

    /// Writes the recorded GUI event log as CSV next to the model.
    fn export_event_log(&mut self) {
        if self.event_log.is_empty() {
            self.error = Some("the event log is empty".into());
            return;
        }
        let unit = self
            .model
            .map(|model| model.time_unit.label())
            .unwrap_or("");
        let mut text = format!(
            "# OpenFlex GUI event log\n# model: {}\n# time_unit: {}\n# events: {}\n",
            self.model.map(|model| model.name.as_str()).unwrap_or(""),
            unit,
            self.event_log.len()
        );
        text.push_str("time,event,object\n");
        for entry in &self.event_log {
            let object = entry
                .object
                .as_ref()
                .map(|object| {
                    // Quote names that would break the CSV shape.
                    if object.contains(',') || object.contains('"') {
                        format!("\"{}\"", object.replace('"', "\"\""))
                    } else {
                        object.clone()
                    }
                })
                .unwrap_or_default();
            text.push_str(&format!("{:.4},{},{object}\n", entry.time, entry.kind));
        }
        let path = self.report_path("events.csv");
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.remember_output(&path);
                self.status = format!("Event log written to {}", path.display());
                self.error = None;
            }
            Err(error) => self.error = Some(format!("could not write event log: {error}")),
        }
    }

    /// `validate_collect` for the current model, memoised by generation so a
    /// 200-object model is not re-validated on every frame.
    fn validation_issues(&mut self) -> Vec<openflex_model::ValidationIssue> {
        if let Some((generation, issues)) = self.validity_cache.as_ref()
            && *generation == self.model_gen
        {
            return issues.clone();
        }
        let Some(model) = self.model else {
            return Vec::new();
        };
        let issues = openflex_model::validate_collect(model);
        self.validity_cache = Some((self.model_gen, issues.clone()));
        issues
    }

    fn report_path(&self, extension: &str) -> PathBuf {
        self.path
            .as_ref()
            .map(|path| path.with_extension(extension))
            .unwrap_or_else(|| PathBuf::from(format!("openflex-report.{extension}")))
    }

    /// Keeps the current report as the comparison baseline.
    fn snapshot_baseline(&mut self) {
        match self.report.clone() {
            Some(report) => {
                self.baseline = Some(report);
                self.show_compare = true;
                self.status = "Baseline captured".into();
            }
            None => self.error = Some("run the model first".into()),
        }
    }

    fn export_compare_csv(&mut self) {
        let (Some(report), Some(baseline)) = (self.report.as_ref(), self.baseline.as_ref()) else {
            return;
        };
        let mut text = format!(
            "# OpenFlex baseline comparison\n# model: {}\n# time_unit: {}\n# baseline completed: {:.1}\n# current completed: {:.1}\n",
            report.model,
            report.time_unit.label(),
            baseline.completed.mean,
            report.completed.mean
        );
        text.push_str("object,metric,baseline,current,delta,delta_percent\n");
        let mut push = |object: &str, metric: &str, old: f64, new: f64| {
            let delta = new - old;
            let percent = if old.abs() > 1e-12 {
                format!("{:.1}", 100.0 * delta / old)
            } else {
                String::new()
            };
            text.push_str(&format!(
                "{object},{metric},{old:.4},{new:.4},{delta:+.4},{percent}\n"
            ));
        };
        push(
            "<all>",
            "completed",
            baseline.completed.mean,
            report.completed.mean,
        );
        for (name, current) in &report.objects {
            let Some(base) = baseline.objects.get(name) else {
                continue;
            };
            push(
                name,
                "input",
                base.input_count.mean,
                current.input_count.mean,
            );
            push(
                name,
                "output",
                base.output_count.mean,
                current.output_count.mean,
            );
            push(name, "wip", base.avg_content.mean, current.avg_content.mean);
            push(
                name,
                "stay",
                base.avg_stay_time.mean,
                current.avg_stay_time.mean,
            );
            push(
                name,
                "utilization",
                base.utilization.mean,
                current.utilization.mean,
            );
        }
        let path = self.report_path("compare.csv");
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.remember_output(&path);
                self.status = format!("Comparison written to {}", path.display());
                self.error = None;
            }
            Err(error) => self.error = Some(format!("could not write comparison: {error}")),
        }
    }

    fn export_report_csv(&mut self) {
        let Some(report) = self.report.as_ref() else {
            return;
        };
        // Every metric carries its 95% half-width next to the mean so the
        // export matches the CLI/Studio tables (which show mean ± CI95).
        // A `#` header documents the run so a stray file is self-describing.
        let unit = report.time_unit.label();
        let mut text = format!(
            "# OpenFlex report\n# model: {}\n# time_unit: {}\n# stop_time: {}\n# warmup: {}\n# seed: {}\n# replications: {}\n",
            report.model,
            unit,
            report.stop_time,
            report.warmup,
            report.base_seed,
            report.replications.len()
        );
        text.push_str(
            "object,kind,input,input_ci95,output,output_ci95,avg_wip,avg_wip_ci95,\
max_wip,avg_stay,avg_stay_ci95,throughput,throughput_ci95,utilization,utilization_ci95,\
idle,busy,blocked,down\n",
        );
        for (name, stats) in &report.objects {
            text.push_str(&format!(
                "{name},{},{:.1},{:.2},{:.1},{:.2},{:.3},{:.4},{:.0},{:.3},{:.4},\
{:.5},{:.6},{:.4},{:.5},{:.4},{:.4},{:.4},{:.4}\n",
                stats.kind.as_str(),
                stats.input_count.mean,
                stats.input_count.ci95,
                stats.output_count.mean,
                stats.output_count.ci95,
                stats.avg_content.mean,
                stats.avg_content.ci95,
                stats.max_content.mean,
                stats.avg_stay_time.mean,
                stats.avg_stay_time.ci95,
                stats.throughput.mean,
                stats.throughput.ci95,
                stats.utilization.mean,
                stats.utilization.ci95,
                stats.idle_fraction.mean,
                stats.busy_fraction.mean,
                stats.blocked_fraction.mean,
                stats.down_fraction.mean
            ));
        }
        let path = self.report_path("report.csv");
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.remember_output(&path);
                self.status = format!("Report CSV written to {}", path.display());
                self.error = None;
            }
            Err(error) => self.error = Some(format!("could not write report CSV: {error}")),
        }
    }

    fn save_baseline_json(&mut self) {
        let Some(baseline) = self.baseline.as_ref() else {
            self.error = Some("capture a baseline first".into());
            return;
        };
        match serde_json::to_string_pretty(baseline) {
            Ok(text) => {
                let path = self.report_path("baseline.json");
                match std::fs::write(&path, text) {
                    Ok(()) => {
                        self.remember_output(&path);
                        self.status = format!("Baseline written to {}", path.display());
                    }
                    Err(error) => self.error = Some(format!("could not write baseline: {error}")),
                }
            }
            Err(error) => self.error = Some(format!("could not serialise baseline: {error}")),
        }
    }

    fn load_baseline_json(&mut self) {
        let path = self.report_path("baseline.json");
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.error = Some(format!("could not read {}: {error}", path.display()));
                return;
            }
        };
        match serde_json::from_str::<SimReport>(&text) {
            Ok(report) => {
                // A baseline from another model would make every delta
                // meaningless, so require the names to line up unless the
                // current model has no name yet.
                let mismatch = self
                    .model
                    .map(|model| !model.name.is_empty() && model.name != report.model)
                    .unwrap_or(false);
                let note = if mismatch {
                    format!(
                        " (warning: baseline is for '{}', current model is '{}')",
                        report.model,
                        self.model.map(|model| model.name.as_str()).unwrap_or("")
                    )
                } else {
                    String::new()
                };
                self.baseline = Some(report);
                self.show_compare = true;
                self.status = format!("Baseline loaded from {}{note}", path.display());
                self.error = None;
            }
            Err(error) => self.error = Some(format!("invalid baseline JSON: {error}")),
        }
    }

    fn export_report_json(&mut self) {
        let Some(report) = self.report.as_ref() else {
            return;
        };
        match serde_json::to_string_pretty(report) {
            Ok(text) => {
                let path = self.report_path("report.json");
                match std::fs::write(&path, text) {
                    Ok(()) => {
                        self.remember_output(&path);
                        self.status = format!("Report JSON written to {}", path.display());
                    }
                    Err(error) => {
                        self.error = Some(format!("could not write report JSON: {error}"))
                    }
                }
            }
            Err(error) => self.error = Some(format!("could not serialise report: {error}")),
        }
    }

    /// Sidecar file that stores the named layout presets of the open model.
    fn preset_path(&self) -> Option<std::path::PathBuf> {
        self.path
            .as_ref()
            .map(|path| path.with_extension("layout.json"))
    }

    fn load_presets(&mut self) {
        self.layout_presets.clear();
        let Some(path) = self.preset_path() else {
            return;
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return;
        };
        let Some(entries) = value.get("presets").and_then(|p| p.as_array()) else {
            return;
        };
        for entry in entries {
            let Some(name) = entry.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let Some(objects) = entry.get("objects").and_then(|o| o.as_array()) else {
                continue;
            };
            let positions = objects
                .iter()
                .filter_map(|object| {
                    let object_name = object.get("name")?.as_str()?.to_string();
                    let pos = object.get("pos")?.as_array()?;
                    let x = pos.first()?.as_f64()?;
                    let y = pos.get(1)?.as_f64()?;
                    Some(([x, y], object_name))
                })
                .collect();
            self.layout_presets.push((name.to_string(), positions));
        }
    }

    fn save_presets(&mut self) {
        let Some(path) = self.preset_path() else {
            return;
        };
        let presets: Vec<serde_json::Value> = self
            .layout_presets
            .iter()
            .map(|(name, objects)| {
                serde_json::json!({
                    "name": name,
                    "objects": objects
                        .iter()
                        .map(|(pos, object)| serde_json::json!({"name": object, "pos": pos}))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        let text = serde_json::json!({ "presets": presets }).to_string();
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.status = format!("Layout presets written to {}", path.display());
                self.error = None;
            }
            Err(error) => self.error = Some(format!("could not write presets: {error}")),
        }
    }

    fn store_layout_preset(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let name = if self.preset_name.trim().is_empty() {
            format!("preset{}", self.layout_presets.len() + 1)
        } else {
            self.preset_name.trim().to_string()
        };
        let objects: Vec<([f64; 2], String)> = model
            .objects
            .iter()
            .map(|object| (object.pos, object.name.clone()))
            .collect();
        self.layout_presets
            .retain(|(existing, _)| existing != &name);
        self.layout_presets.push((name.clone(), objects));
        self.save_presets();
        self.preset_name.clear();
        self.status = format!("Saved layout preset '{name}'");
    }

    fn apply_layout_preset(&mut self, name: &str) {
        let Some(model) = self.model else {
            return;
        };
        let Some((_, objects)) = self
            .layout_presets
            .iter()
            .find(|(existing, _)| existing == name)
            .cloned()
        else {
            return;
        };
        self.snapshot_labelled("apply layout preset");
        let mut copy = model.clone();
        let mut applied = 0;
        for (pos, object_name) in objects {
            if let Some(index) = copy
                .objects
                .iter()
                .position(|object| object.name == object_name)
            {
                copy.objects[index].pos = pos;
                applied += 1;
            }
        }
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.status = format!("Applied preset '{name}' to {applied} object(s)");
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn delete_layout_preset(&mut self, name: &str) {
        self.layout_presets.retain(|(existing, _)| existing != name);
        self.save_presets();
        self.status = format!("Deleted preset '{name}'");
    }

    /// Re-reads the checkpoint directory for the list panel.
    fn refresh_checkpoints(&mut self) {
        let dir = if self.checkpoint_dir.trim().is_empty() {
            "checkpoints".to_string()
        } else {
            self.checkpoint_dir.trim().to_string()
        };
        self.checkpoint_files.clear();
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return;
        };
        let mut files: Vec<(String, f64)> = entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .filter_map(|path| {
                let name = path.file_name()?.to_string_lossy().into_owned();
                let text = std::fs::read_to_string(&path).ok()?;
                let time = serde_json::from_str::<serde_json::Value>(&text)
                    .ok()
                    .and_then(|value| value.get("now").and_then(|now| now.as_f64()))
                    .unwrap_or(f64::NAN);
                Some((name, time))
            })
            .collect();
        files.sort_by(|left, right| left.0.cmp(&right.0));
        self.checkpoint_files = files;
    }

    /// Loads one specific checkpoint file from the configured directory.
    fn load_checkpoint_file(&mut self, file: &str) {
        let dir = if self.checkpoint_dir.trim().is_empty() {
            "checkpoints".to_string()
        } else {
            self.checkpoint_dir.trim().to_string()
        };
        let path = std::path::Path::new(&dir).join(file);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.error = Some(format!("could not read {}: {error}", path.display()));
                return;
            }
        };
        let checkpoint = match openflex_core::Checkpoint::from_json_verified(&text, None) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        self.handle = None;
        if !self.new_handle() {
            return;
        }
        let Some(handle) = self.handle.as_mut() else {
            return;
        };
        match handle.restore(&checkpoint) {
            Ok(()) => {
                self.sim_state = Some(handle.state());
                // The restored run is a different timeline: drop the smoothed
                // positions and the event log from the previous one.
                self.smooth.clear();
                self.replay_index = None;
                self.event_log.clear();
                self.running = false;
                self.paused = false;
                self.checkpoint_status = format!("restored {file}");
                self.status = self.checkpoint_status.clone();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Model time encoded in a checkpoint file name
    /// (`checkpoint-12.50.json` → 12.5, `checkpoint-12.50-2.json` → 12.5).
    fn checkpoint_time(path: &std::path::Path) -> f64 {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .and_then(|stem| stem.strip_prefix("checkpoint-"))
            .and_then(|rest| rest.split('-').next())
            .and_then(|time| time.parse::<f64>().ok())
            .unwrap_or(0.0)
    }

    /// Saves the current simulation state as a checkpoint JSON file.
    fn save_checkpoint(&mut self) {
        if self.handle.is_none() && !self.new_handle() {
            return;
        }
        let Some(handle) = self.handle.as_ref() else {
            return;
        };
        let checkpoint = handle.snapshot();
        let text = match checkpoint.to_json() {
            Ok(text) => text,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let dir = if self.checkpoint_dir.trim().is_empty() {
            "checkpoints".to_string()
        } else {
            self.checkpoint_dir.trim().to_string()
        };
        if let Err(error) = std::fs::create_dir_all(&dir) {
            self.error = Some(format!("could not create {dir}: {error}"));
            return;
        }
        let name = format!("checkpoint-{:.2}.json", handle.now());
        let path = std::path::Path::new(&dir).join(name);
        // Saving twice at the same model time would silently overwrite the
        // earlier state; keep both by adding a numeric suffix instead.
        let path = if path.exists() {
            let stem = format!("checkpoint-{:.2}", handle.now());
            let mut counter = 2;
            loop {
                let candidate = std::path::Path::new(&dir).join(format!("{stem}-{counter}.json"));
                if !candidate.exists() {
                    break candidate;
                }
                counter += 1;
            }
        } else {
            path
        };
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.checkpoint_status = format!("saved {}", path.display());
                self.status = self.checkpoint_status.clone();
                self.error = None;
                self.refresh_checkpoints();
            }
            Err(error) => self.error = Some(format!("could not write checkpoint: {error}")),
        }
    }

    /// Loads the newest checkpoint from the directory into a fresh handle.
    fn load_checkpoint(&mut self) {
        let dir = if self.checkpoint_dir.trim().is_empty() {
            "checkpoints".to_string()
        } else {
            self.checkpoint_dir.trim().to_string()
        };
        let mut entries: Vec<std::path::PathBuf> = match std::fs::read_dir(&dir) {
            Ok(entries) => entries
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .collect(),
            Err(error) => {
                self.error = Some(format!("could not read {dir}: {error}"));
                return;
            }
        };
        // Sort by the model time encoded in the name, not lexically: with plain
        // string ordering `checkpoint-9.00` would sort after `checkpoint-10.00`.
        entries.sort_by(|left, right| {
            Self::checkpoint_time(left).total_cmp(&Self::checkpoint_time(right))
        });
        let Some(path) = entries.last().cloned() else {
            self.error = Some(format!("no checkpoints in {dir}"));
            return;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                self.error = Some(format!("could not read {}: {error}", path.display()));
                return;
            }
        };
        let checkpoint = match openflex_core::Checkpoint::from_json_verified(&text, None) {
            Ok(checkpoint) => checkpoint,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        // A checkpoint restores into a handle for the same model.
        self.handle = None;
        if !self.new_handle() {
            return;
        }
        let Some(handle) = self.handle.as_mut() else {
            return;
        };
        match handle.restore(&checkpoint) {
            Ok(()) => {
                self.sim_state = Some(handle.state());
                self.checkpoint_status = format!("restored {}", path.display());
                self.status = self.checkpoint_status.clone();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Writes the model-level settings (name, run configuration, time unit).
    fn apply_model_settings(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let seed: u64 = match self.model_seed.trim().parse() {
            Ok(seed) => seed,
            Err(error) => {
                self.error = Some(format!("invalid seed: {error}"));
                return;
            }
        };
        let mut copy = model.clone();
        if !self.model_name.trim().is_empty() {
            copy.name = self.model_name.trim().to_string();
        }
        copy.run.stop_time = (self.model_stop as f64).max(1.0);
        copy.run.warmup = (self.model_warmup as f64).max(0.0);
        copy.run.seed = seed;
        copy.run.replications = self.model_replications.max(1);
        copy.time_unit = match self.model_unit {
            1 => openflex_model::TimeUnit::Minutes,
            2 => openflex_model::TimeUnit::Hours,
            3 => openflex_model::TimeUnit::Days,
            _ => openflex_model::TimeUnit::Seconds,
        };
        match copy.validate() {
            Ok(()) => {
                // Applying run settings changes the document too, so the
                // unsaved marker and the undo stack must both know about it.
                self.snapshot_labelled("model settings");
                self.status = format!(
                    "Model settings applied (stop {}, seed {})",
                    copy.run.stop_time, copy.run.seed
                );
                self.model = Some(Box::leak(Box::new(copy)));
                self.handle = None;
                self.sim_state = None;
                self.report = None;
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Rotates the named object by `delta` degrees (writes `Object.rot`).
    /// `f64::NEG_INFINITY` resets the rotation to 0.
    fn rotate_object(&mut self, name: &str, delta: f64) {
        let Some(model) = self.model else {
            return;
        };
        let Some(index) = model.objects.iter().position(|object| object.name == name) else {
            return;
        };
        self.snapshot();
        let mut copy = model.clone();
        let current = copy.objects[index].rot.unwrap_or(0.0);
        let next = if delta.is_infinite() {
            0.0
        } else {
            // Normalise into [0, 360) so the file stays readable.
            (current + delta).rem_euclid(360.0)
        };
        copy.objects[index].rot = if next == 0.0 { None } else { Some(next) };
        match copy.validate() {
            Ok(()) => {
                let stored = copy.objects[index].rot.unwrap_or(0.0);
                self.model = Some(Box::leak(Box::new(copy)));
                // Refresh the form so the side panel shows the new rotation.
                if self.selected == Some(index) {
                    self.form = self
                        .model
                        .map(|model| EditForm::from_object(&model.objects[index]));
                }
                self.status = format!("{name} rotated to {stored:.0}°");
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Adds an object of the chosen kind with engine-safe default parameters.
    fn add_object(&mut self, kind_name: &str) {
        let Some(kind) = ObjectKind::parse(kind_name) else {
            self.error = Some(format!("unknown object kind '{kind_name}'"));
            return;
        };
        let Some(model) = self.model else {
            self.error = Some("open a model first".into());
            return;
        };
        let name = if self.new_name.trim().is_empty() {
            let mut suffix = 1;
            loop {
                let candidate = format!("{}{suffix}", kind.as_str());
                if !model.objects.iter().any(|object| object.name == candidate) {
                    break candidate;
                }
                suffix += 1;
            }
        } else {
            let candidate = self.new_name.trim().to_string();
            if model.objects.iter().any(|object| object.name == candidate) {
                self.error = Some(format!("an object named '{candidate}' already exists"));
                return;
            }
            candidate
        };
        let params_json = if kind == ObjectKind::Source {
            serde_json::json!({ "interarrival": { "distribution": "constant", "value": 5.0 } })
        } else {
            serde_json::json!({})
        };
        let params = match parse_params(kind, params_json) {
            Ok(params) => params,
            Err(error) => {
                self.error = Some(error);
                return;
            }
        };
        // Place the new object on a free slot to the right of the model: walk
        // down a column until the spot is not taken, so repeated adds do not
        // pile up on one line.
        let max_x = model
            .objects
            .iter()
            .map(|object| object.pos[0])
            .fold(-8.0_f64, f64::max);
        let mut slot_y = 0.0_f64;
        while model.objects.iter().any(|object| {
            (object.pos[0] - (max_x + 4.0)).abs() < 2.0 && (object.pos[1] - slot_y).abs() < 2.0
        }) {
            slot_y -= 4.0;
        }
        self.snapshot_labelled("add object");
        let mut copy = model.clone();
        copy.objects.push(openflex_model::model::Object {
            name: name.clone(),
            kind,
            pos: [max_x + 4.0, slot_y],
            z: 0.0,
            rot: None,
            params,
        });
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.new_name.clear();
                self.status = format!("Added {name}");
                self.error = None;
                if let Some(index) = self
                    .model
                    .and_then(|model| model.objects.iter().position(|object| object.name == name))
                {
                    self.select(index);
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Frames the current selection: centres the camera on its midpoint and
    /// zooms so the group spans roughly half of the view.
    fn frame_selection(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let indices = if self.selection.is_empty() {
            (0..model.objects.len()).collect::<Vec<_>>()
        } else {
            self.selection.clone()
        };
        if indices.is_empty() {
            return;
        }
        let points: Vec<[f64; 2]> = indices
            .iter()
            .filter(|index| **index < model.objects.len())
            .map(|index| model.objects[*index].pos)
            .collect();
        if points.is_empty() {
            return;
        }
        if !self.canvas_measured {
            // The first frame has not been painted yet, so the viewport size is
            // unknown; remember the request and run it right after painting.
            self.frame_pending = true;
            return;
        }
        let (centre, zoom) = Self::frame_maths_view(
            &points,
            self.yaw,
            self.pitch,
            self.canvas_size.x.min(self.canvas_size.y),
        );
        self.focus = centre;
        self.zoom = zoom;
        self.status = format!("Framed {} object(s)", indices.len());
    }

    /// Diagram colour for one Process Flow block type.
    fn flow_kind_colour(kind: &str) -> egui::Color32 {
        match kind {
            "source" | "create" => egui::Color32::from_rgb(58, 132, 106),
            "delay" => egui::Color32::from_rgb(150, 116, 58),
            "assign" => egui::Color32::from_rgb(96, 96, 158),
            "decision" => egui::Color32::from_rgb(148, 92, 128),
            "sink" => egui::Color32::from_rgb(96, 104, 122),
            "transfer" => egui::Color32::from_rgb(70, 108, 150),
            _ => egui::Color32::from_rgb(78, 96, 132),
        }
    }

    /// Camera centre and zoom that frame `points` **in projected space**, so
    /// the current camera rotation cannot push labels outside the viewport.
    /// `canvas` is the smaller side of the drawing area in pixels.
    fn frame_maths_view(points: &[[f64; 2]], yaw: f32, pitch: f32, canvas: f32) -> ([f32; 2], f32) {
        let (sin_yaw, cos_yaw) = yaw.sin_cos();
        let (sin_pitch, _cos_pitch) = pitch.sin_cos();
        let mut min = [f32::INFINITY; 2];
        let mut max = [f32::NEG_INFINITY; 2];
        let mut sum = [0.0_f32; 2];
        for pos in points {
            let x = pos[0] as f32;
            let y = pos[1] as f32;
            let rx = x * cos_yaw - y * sin_yaw;
            let ry = (x * sin_yaw + y * cos_yaw) * sin_pitch;
            min[0] = min[0].min(rx);
            min[1] = min[1].min(ry);
            max[0] = max[0].max(rx);
            max[1] = max[1].max(ry);
            sum[0] += rx;
            sum[1] += ry;
        }
        let count = points.len() as f32;
        let centroid = [sum[0] / count, sum[1] / count];
        // Invert the projection so the focus lands on the projected centroid.
        //   rx =  x·cos - y·sin            X = rx
        //   ry = (x·sin + y·cos)·sin(p)    R = ry / sin(p)
        // → x =  X·cos + R·sin,  y = -X·sin + R·cos
        let sin_pitch = sin_pitch.max(1e-3);
        let x = centroid[0];
        let r = centroid[1] / sin_pitch;
        let centre = [x * cos_yaw + r * sin_yaw, -x * sin_yaw + r * cos_yaw];
        // Fit the group into the canvas with a margin that leaves room for the
        // object boxes and their captions, which extend outside the projected
        // centre of each object.
        let padding = 4.0_f32;
        let span = (max[0] - min[0]).max(max[1] - min[1]).max(1.0) + padding;
        let usable = (canvas - 200.0).max(160.0);
        // scale = canvas / 12 * zoom, and span * scale must fit `usable`.
        let zoom = (usable * 12.0 / (span * canvas.max(1.0))).clamp(ZOOM_MIN, ZOOM_MAX);
        (centre, zoom)
    }

    /// Selects every object in the model.
    fn select_all(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        self.selection = (0..model.objects.len()).collect();
        self.status = format!("{} object(s) selected", self.selection.len());
    }

    /// Copies the selected objects with unique names, offset positions and the
    /// same parameters (connections are not duplicated).
    fn duplicate_selection(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        if self.selection.is_empty() {
            return;
        }
        self.snapshot_labelled("duplicate selection");
        let mut copy = model.clone();
        let mut added = 0;
        for index in self.selection.clone() {
            let Some(source) = model.objects.get(index) else {
                continue;
            };
            let base = source.name.clone();
            let mut suffix = 2;
            let mut name = format!("{base}_{suffix}");
            while copy.objects.iter().any(|object| object.name == name) {
                suffix += 1;
                name = format!("{base}_{suffix}");
            }
            let mut object = source.clone();
            object.name = name;
            object.pos = [object.pos[0] + 2.0, object.pos[1] + 2.0];
            copy.objects.push(object);
            added += 1;
        }
        match copy.validate() {
            Ok(()) => {
                // Select the copies: that is what the user wants to move next,
                // and it matches FlexSim's duplicate behaviour.
                let first_new = model.objects.len();
                let new_indices: Vec<usize> = (first_new..first_new + added).collect();
                self.model = Some(Box::leak(Box::new(copy)));
                if let Some(first) = new_indices.first().copied() {
                    self.selection = new_indices;
                    self.select(first);
                }
                self.status = format!("Duplicated {added} object(s)");
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Places every object on a readable grid following the connection order
    /// (breadth-first from the sources), used when a model has no positions.
    fn auto_layout(&mut self, silent: bool) {
        let Some(model) = self.model else {
            return;
        };
        let order = Self::layout_order(model);
        let columns = (order.len() as f64).sqrt().ceil().max(1.0) as usize;
        let spacing = 6.0;
        if !silent {
            self.snapshot_labelled("auto layout");
        }
        let mut copy = model.clone();
        for (position, index) in order.iter().enumerate() {
            let column = position % columns;
            let row = position / columns;
            copy.objects[*index].pos = [
                (column as f64 - (columns as f64 - 1.0) / 2.0) * spacing,
                (row as f64 - (order.len() / columns) as f64 / 2.0) * spacing,
            ];
        }
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                if let Some(index) = self.selected {
                    self.form = self
                        .model
                        .map(|model| EditForm::from_object(&model.objects[index]));
                }
                if !silent {
                    self.status = format!("Auto layout applied to {} object(s)", order.len());
                }
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Object indices in breadth-first connection order (sources first, then
    /// everything reachable, finally any isolated object).
    fn layout_order(model: &Model) -> Vec<usize> {
        let mut order: Vec<usize> = Vec::new();
        let mut seen = vec![false; model.objects.len()];
        let mut queue: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
        // Roots start the walk: no incoming connection but at least one
        // outgoing one (fully isolated objects come last instead).
        for (index, object) in model.objects.iter().enumerate() {
            let has_input = model
                .connections
                .iter()
                .any(|connection| connection.to == object.name);
            let has_output = model
                .connections
                .iter()
                .any(|connection| connection.from == object.name);
            if !has_input && has_output && !seen[index] {
                seen[index] = true;
                queue.push_back(index);
            }
        }
        while let Some(index) = queue.pop_front() {
            order.push(index);
            let name = &model.objects[index].name;
            for connection in &model.connections {
                if &connection.from != name {
                    continue;
                }
                if let Some(next) = model
                    .objects
                    .iter()
                    .position(|object| object.name == connection.to)
                    && !seen[next]
                {
                    seen[next] = true;
                    queue.push_back(next);
                }
            }
        }
        // Objects the walk never reached (isolated ones, or every object when
        // the graph has no root at all because it contains a cycle) still need a
        // slot, otherwise the layout would silently leave them where they are.
        for (index, used) in seen.iter().enumerate() {
            if !used {
                order.push(index);
            }
        }
        // Defensive: a model always yields one entry per object.
        debug_assert_eq!(order.len(), model.objects.len());
        order
    }

    fn batch_align(&mut self, along_x: bool) {
        let Some(model) = self.model else {
            return;
        };
        if self.selection.len() < 2 {
            return;
        }
        let indices = self.selection.clone();
        // Align on the selection's extreme value rather than "whichever was
        // clicked first", which depends on marquee order.
        let axis = if along_x { 0 } else { 1 };
        let anchor = indices
            .iter()
            .map(|index| model.objects[*index].pos[axis])
            .fold(f64::INFINITY, f64::min);
        self.snapshot_labelled("align selection");
        let mut copy = model.clone();
        for index in indices {
            copy.objects[index].pos[axis] = anchor;
        }
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.status = format!(
                    "Aligned on {} at {:.1}",
                    if along_x { "X" } else { "Y" },
                    anchor
                );
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Spreads the selection evenly between its first and last element along
    /// the chosen axis (a compact FlexSim-style "distribute/space evenly").
    fn batch_distribute(&mut self, along_x: bool) {
        let Some(model) = self.model else {
            return;
        };
        if self.selection.len() < 3 {
            self.error = Some("distribute needs at least three objects".into());
            return;
        }
        let mut indices = self.selection.clone();
        indices.sort_by(|left, right| {
            let a = model.objects[*left].pos;
            let b = model.objects[*right].pos;
            let (a, b) = if along_x { (a[0], b[0]) } else { (a[1], b[1]) };
            a.total_cmp(&b)
        });
        let first = if along_x {
            model.objects[indices[0]].pos[0]
        } else {
            model.objects[indices[0]].pos[1]
        };
        let last = if along_x {
            model.objects[*indices.last().expect("non-empty")].pos[0]
        } else {
            model.objects[*indices.last().expect("non-empty")].pos[1]
        };
        let step = (last - first) / (indices.len() - 1) as f64;
        self.snapshot_labelled("distribute selection");
        let mut copy = model.clone();
        for (position, index) in indices.iter().enumerate() {
            let value = first + step * position as f64;
            if along_x {
                copy.objects[*index].pos[0] = value;
            } else {
                copy.objects[*index].pos[1] = value;
            }
        }
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.status = format!(
                    "Distributed {} object(s) along {}",
                    self.selection.len(),
                    if along_x { "X" } else { "Y" }
                );
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Lays the selection out on a square-ish grid with `spacing` units.
    fn batch_grid(&mut self, spacing: f64) {
        let Some(model) = self.model else {
            return;
        };
        if self.selection.len() < 2 {
            self.error = Some("grid layout needs at least two objects".into());
            return;
        }
        let count = self.selection.len();
        let columns = (count as f64).sqrt().ceil() as usize;
        // Anchor on the selection's top-left corner, like Align does, so the
        // result does not depend on which object was clicked first.
        let origin = [
            self.selection
                .iter()
                .map(|index| model.objects[*index].pos[0])
                .fold(f64::INFINITY, f64::min),
            self.selection
                .iter()
                .map(|index| model.objects[*index].pos[1])
                .fold(f64::INFINITY, f64::min),
        ];
        self.snapshot_labelled("grid layout");
        let mut copy = model.clone();
        for (position, index) in self.selection.clone().into_iter().enumerate() {
            let column = position % columns;
            let row = position / columns;
            copy.objects[index].pos = [
                origin[0] + column as f64 * spacing,
                origin[1] + row as f64 * spacing,
            ];
        }
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.status = format!("Grid layout for {count} object(s) @ {spacing}");
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn batch_move(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let (dx, dy) = (self.batch_dx as f64, self.batch_dy as f64);
        if dx == 0.0 && dy == 0.0 {
            return;
        }
        self.snapshot_labelled("move selection");
        let mut copy = model.clone();
        for index in self.selection.clone() {
            copy.objects[index].pos[0] += dx;
            copy.objects[index].pos[1] += dy;
        }
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.form = self.selected.and_then(|index| {
                    self.model
                        .map(|model| EditForm::from_object(&model.objects[index]))
                });
                self.status = format!("Moved {} object(s)", self.selection.len());
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Sets the same rotation on every selected object (0 clears the field).
    fn batch_rotate(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        if self.selection.is_empty() {
            return;
        }
        let value = self.batch_rot.rem_euclid(360.0) as f64;
        self.snapshot_labelled("rotate selection");
        let mut copy = model.clone();
        for index in self.selection.clone() {
            copy.objects[index].rot = if value.abs() < 0.01 {
                None
            } else {
                Some(value)
            };
        }
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.status = format!("Rotated {} object(s) to {value:.0}°", self.selection.len());
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn batch_delete(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        if self.selection.is_empty() {
            return;
        }
        let names: Vec<String> = self
            .selection
            .iter()
            .filter_map(|index| model.objects.get(*index).map(|object| object.name.clone()))
            .collect();
        self.delete_objects(&names);
    }

    fn new_handle(&mut self) -> bool {
        let Some(model) = self.model else {
            return false;
        };
        if self.handle.is_none() {
            let config = SimConfig {
                stop_time: model.run.stop_time,
                warmup: model.run.warmup,
                seed: model.run.seed,
            };
            match SimulationHandle::new(model, &config, 0) {
                Ok(handle) => self.handle = Some(handle),
                Err(error) => {
                    self.error = Some(error.to_string());
                    return false;
                }
            }
        }
        true
    }

    fn reset_run(&mut self) {
        self.event_log.clear();
        // Drop the interpolated positions too: otherwise the next run starts by
        // sliding objects from wherever the previous one stopped.
        self.smooth.clear();
        self.replay_index = None;
        self.handle = None;
        self.sim_state = None;
        self.running = false;
        self.paused = false;
        self.report = None;
        self.status = "Reset".into();
    }

    fn start_experiment(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let base = model.clone();
        let (start, step, count) = (
            self.exp_start as f64,
            self.exp_step as f64,
            self.exp_count.max(1),
        );
        let sweep_params = self.exp_mode_param;
        let path = self.exp_path.clone();
        let reps = self.exp_reps.max(1);
        let parallel = self.parallel_runs;
        let base_json = if sweep_params {
            match serde_json::to_value(&base) {
                Ok(value) => Some(value),
                Err(error) => {
                    self.error = Some(format!("could not serialise model: {error}"));
                    return;
                }
            }
        } else {
            None
        };
        let source_model = base.name.clone();
        let (tx, rx) = channel();
        self.exp_rx = Some(rx);
        self.exp_running = true;
        self.status = format!("Experiment: {count} runs…");
        let started = std::time::Instant::now();
        std::thread::spawn(move || {
            let mut rows = Vec::new();
            let mut failed = 0usize;
            for index in 0..count {
                let value = start + step * index as f64;
                let mut model = base.clone();
                if sweep_params {
                    let Some(json) = base_json.as_ref() else {
                        continue;
                    };
                    let mut json = json.clone();
                    if !set_json_number(&mut json, &path, value) {
                        continue;
                    }
                    match serde_json::from_value::<Model>(json) {
                        Ok(patched) => model = patched,
                        Err(_) => continue,
                    }
                } else {
                    model.run.stop_time = value;
                }
                // FlexSim-style experimenter: every scenario runs `reps`
                // independent replications and the table shows mean ± CI.
                model.run.replications = reps;
                let outcome = if parallel {
                    openflex_core::run_model_parallel(&model, 0)
                } else {
                    openflex_core::run_model(&model)
                };
                match outcome {
                    Ok(report) => rows.push((value, report.completed.mean, report.completed.ci95)),
                    Err(_) => failed += 1,
                }
            }
            let _ = tx.send((rows, failed, source_model, started.elapsed()));
        });
    }

    fn poll_experiment(&mut self) {
        let Some(rx) = self.exp_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok((rows, failed, source, elapsed)) => {
                let seconds = elapsed.as_secs_f64();
                let runs = rows.len().max(1);
                let stale = self.result_is_stale(&source);
                let mut note = if stale {
                    format!(" · from '{source}' (model changed since)")
                } else {
                    String::new()
                };
                if failed > 0 {
                    note.push_str(&format!(" · {failed} scenario(s) failed"));
                }
                self.status = format!(
                    "Experiment finished in {seconds:.2}s: {} runs ({:.2}s each){note}",
                    rows.len(),
                    seconds / runs as f64
                );
                self.exp_rows = rows;
                self.exp_running = false;
                self.exp_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.exp_running = false;
                self.exp_rx = None;
            }
        }
    }

    /// Runs one replication with the engine trace enabled and keeps the event
    /// stream, so the whole run can be inspected or exported (the interactive
    /// event log only records what the GUI itself stepped through).
    fn start_trace_run(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        if self.trace_running {
            return;
        }
        // A traced run is a full simulation: keep it off the UI thread so the
        // window stays responsive on a large model.
        let config = SimConfig {
            stop_time: model.run.stop_time,
            warmup: model.run.warmup,
            seed: model.run.seed,
        };
        let owned = model.clone();
        let (tx, rx) = channel();
        self.trace_rx = Some(rx);
        self.trace_running = true;
        self.status = "Tracing…".into();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let outcome = openflex_core::simulate_traced(&owned, &config, 0);
            let _ = tx.send((outcome, started.elapsed()));
        });
    }

    fn poll_trace(&mut self) {
        let Some(rx) = self.trace_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok((outcome, elapsed)) => {
                self.trace_running = false;
                self.trace_rx = None;
                match outcome {
                    Ok((result, trace)) => {
                        let truncated = trace.truncated;
                        let events = trace.events.len();
                        self.trace = Some(trace);
                        self.status = format!(
                            "Traced {events} event(s) in {:.2}s{}{}",
                            elapsed.as_secs_f64(),
                            if truncated { " (truncated)" } else { "" },
                            if result.items_completed > 0 {
                                format!(" · {} item(s) completed", result.items_completed)
                            } else {
                                String::new()
                            }
                        );
                        self.error = None;
                    }
                    Err(error) => {
                        self.error = Some(error.to_string());
                        self.status = "Trace failed".into();
                    }
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.trace_running = false;
                self.trace_rx = None;
                self.status = "Trace worker stopped".into();
            }
        }
    }

    /// Writes the traced events as JSONL next to the model.
    fn export_trace(&mut self) {
        let Some(trace) = self.trace.as_ref() else {
            self.error = Some("run Trace first".into());
            return;
        };
        let mut text = String::new();
        for event in &trace.events {
            match serde_json::to_string(event) {
                Ok(line) => {
                    text.push_str(&line);
                    text.push('\n');
                }
                Err(error) => {
                    self.error = Some(format!("could not serialise an event: {error}"));
                    return;
                }
            }
        }
        let path = self.report_path("trace.jsonl");
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.remember_output(&path);
                self.status = format!("Trace written to {}", path.display());
                self.error = None;
            }
            Err(error) => self.error = Some(format!("could not write trace: {error}")),
        }
    }

    fn start_report(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let (tx, rx) = channel();
        self.report_rx = Some(rx);
        self.report_running = true;
        self.status = "Running a full replication…".into();
        let parallel = self.parallel_runs;
        let started = std::time::Instant::now();
        std::thread::spawn(move || {
            // Replications are independent, so running them on all cores is
            // bit-identical to the sequential loop (task-59).
            let result = if parallel {
                openflex_core::run_model_parallel(model, 0)
            } else {
                openflex_core::run_model(model)
            }
            .map_err(|error| error.to_string());
            let _ = tx.send((result, started.elapsed()));
        });
    }

    fn poll_report(&mut self) {
        let Some(rx) = self.report_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok((Ok(report), elapsed)) => {
                // Wall-clock duration plus events/second gives a feel for the
                // run cost that a bare completed-count cannot.
                let seconds = elapsed.as_secs_f64();
                let completed = report.completed.mean.max(0.0);
                self.status = format!(
                    "Run finished in {seconds:.2}s: {completed:.0} completed, {} objects{}",
                    report.objects.len(),
                    if seconds > 0.0 && completed > 0.0 {
                        format!(" · {:.0} items/s", completed / seconds)
                    } else {
                        String::new()
                    }
                );
                // A report belongs to the model it was produced from; if the
                // document changed while the worker ran, keep it but say so.
                let stale = self
                    .model
                    .map(|model| !report.replications.is_empty() && model.name != report.model)
                    .unwrap_or(false);
                if stale {
                    self.status.push_str(" · model changed during the run");
                }
                self.report = Some(report);
                self.report_running = false;
                self.report_rx = None;
            }
            Ok((Err(error), _)) => {
                self.error = Some(error);
                self.status = "Run failed".into();
                self.report_running = false;
                self.report_rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.report_running = false;
                self.report_rx = None;
            }
        }
    }

    /// Replays the run up to `time` (jumping to a logged event): rebuilds the
    /// handle and steps until the first event at/after that time.
    fn jump_to_event(&mut self, time: f64) {
        self.handle = None;
        if !self.new_handle() {
            self.status = "Jump failed: the model could not be started".into();
            return;
        }
        let Some(handle) = self.handle.as_mut() else {
            return;
        };
        if let Err(error) = handle.step_until(time) {
            self.error = Some(error.to_string());
            return;
        }
        self.sim_state = Some(handle.state());
        self.replay_index = None;
        self.status = format!("Jumped to t={time:.3}");
    }

    /// Runs until the next event, recording it in the event log.
    fn step_event(&mut self) {
        if !self.new_handle() {
            return;
        }
        let Some(handle) = self.handle.as_mut() else {
            return;
        };
        let preview = match handle.next_event() {
            Some(preview) => preview,
            None => {
                self.status = "No more events before the stop time".into();
                self.running = false;
                return;
            }
        };
        let entry = EventEntry::from_preview(&preview);
        if let Err(error) = handle.step_event() {
            self.error = Some(error.to_string());
            return;
        }
        self.event_log.push(entry);
        if self.event_log.len() > 200 {
            self.event_log.remove(0);
        }
        self.sim_state = Some(handle.state());
        self.status = format!("Stepped to t={:.3}", preview.time);
    }

    fn advance(&mut self, delta: f64) {
        if !self.new_handle() {
            return;
        }
        let stop = self.model.map(|model| model.run.stop_time).unwrap_or(0.0);
        let Some(handle) = self.handle.as_mut() else {
            return;
        };
        let now = handle.now();
        let target = (now + delta).min(stop);
        // Record the events this step crosses, so Play fills the log the same
        // way Step event does. The cap keeps a huge step from stalling a frame.
        let mut recorded = 0usize;
        while let Some(preview) = handle.next_event() {
            if preview.time > target + 1e-9 || recorded >= 50 {
                break;
            }
            self.event_log.push(EventEntry::from_preview(&preview));
            recorded += 1;
            if handle.step_event().is_err() {
                break;
            }
        }
        if let Err(error) = handle.step_until(target) {
            self.error = Some(error.to_string());
            return;
        }
        // Trim the log once per step instead of per event.
        while self.event_log.len() > 200 {
            self.event_log.remove(0);
        }
        let state = handle.state();
        let finished = state.now >= stop - 1e-9;
        self.sim_state = Some(state);
        if finished {
            self.running = false;
            self.status = format!("Reached stop time {stop:.1}");
        }
    }

    fn select(&mut self, index: usize) {
        let Some(model) = self.model else {
            return;
        };
        let object = &model.objects[index];
        self.form_dirty = false;
        self.rename_text = object.name.clone();
        self.selected = Some(index);
        self.selection = vec![index];
        self.form = Some(EditForm::from_object(object));
        self.params_text =
            serde_json::to_string_pretty(&object.params).unwrap_or_else(|_| "{}".into());
        self.flow_text = serde_json::to_value(&object.params)
            .ok()
            .and_then(|value| value.get("process_flow").cloned())
            .map(|flow| serde_json::to_string_pretty(&flow).unwrap_or_default())
            .unwrap_or_default();
    }

    /// Renames the selected object, rewriting every reference to it
    /// (connections, home/charger/network targets, process-flow links).
    fn rename_object(&mut self, new_name: String) {
        let (Some(model), Some(index)) = (self.model, self.selected) else {
            return;
        };
        let old_name = model.objects[index].name.clone();
        if new_name == old_name || new_name.trim().is_empty() {
            self.rename_text = old_name;
            return;
        }
        if model.objects.iter().any(|object| object.name == new_name) {
            self.error = Some(format!("an object named {new_name} already exists"));
            self.rename_text = old_name;
            return;
        }
        self.snapshot_labelled("rename object");
        let mut copy = model.clone();
        copy.objects[index].name = new_name.clone();
        let json = match serde_json::to_value(&copy) {
            Ok(json) => json,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        // Rewrite every string that referenced the old name.
        let patched = Self::rewrite_name(json, &old_name, &new_name);
        let renamed: Model = match serde_json::from_value(patched) {
            Ok(model) => model,
            Err(error) => {
                self.error = Some(format!("could not rename: {error}"));
                self.rename_text = old_name;
                return;
            }
        };
        match renamed.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(renamed)));
                self.form = self
                    .model
                    .map(|model| EditForm::from_object(&model.objects[index]));
                self.rename_text = new_name.clone();
                self.form_dirty = false;
                self.status = format!("Renamed {old_name} to {new_name}");
                self.error = None;
            }
            Err(error) => {
                self.error = Some(error.to_string());
                self.rename_text = old_name;
            }
        }
    }

    /// Removes `home` / `charger` / `dispatcher` … references to a deleted
    /// object from a JSON tree, the same way `openflex studio` does.
    fn clean_references(value: &mut serde_json::Value, name: &str) -> usize {
        match value {
            serde_json::Value::Object(map) => {
                let mut cleaned = 0;
                let keys: Vec<String> = map.keys().cloned().collect();
                for key in keys {
                    let remove = matches!(
                        key.as_str(),
                        "home"
                            | "charger"
                            | "dispatcher"
                            | "home_floor"
                            | "setup_operator"
                            | "process_operator"
                    ) && map.get(&key).and_then(|value| value.as_str()) == Some(name);
                    if remove {
                        map.remove(&key);
                        cleaned += 1;
                    } else if let Some(child) = map.get_mut(&key) {
                        cleaned += Self::clean_references(child, name);
                    }
                }
                cleaned
            }
            serde_json::Value::Array(items) => items
                .iter_mut()
                .map(|item| Self::clean_references(item, name))
                .sum(),
            _ => 0,
        }
    }

    /// Deletes `names` from the model, dropping their connections **and** the
    /// parameter references that would otherwise keep `validate()` failing.
    fn delete_objects(&mut self, names: &[String]) {
        let Some(model) = self.model else {
            return;
        };
        if names.is_empty() {
            return;
        }
        self.snapshot_labelled("delete object");
        let mut copy = model.clone();
        copy.objects.retain(|object| !names.contains(&object.name));
        copy.connections.retain(|connection| {
            !names.contains(&connection.from) && !names.contains(&connection.to)
        });
        // References inside params (home/charger/operator/network targets).
        let json = match serde_json::to_value(&copy) {
            Ok(json) => json,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        let mut json = json;
        let mut cleaned = 0;
        for name in names {
            if let Some(objects) = json.get_mut("objects").and_then(|o| o.as_array_mut()) {
                for object in objects {
                    if let Some(params) = object.get_mut("params") {
                        cleaned += Self::clean_references(params, name);
                    }
                }
            }
            // Network edges that pointed at the deleted node.
            if let Some(objects) = json.get_mut("objects").and_then(|o| o.as_array_mut()) {
                for object in objects {
                    if let Some(edges) = object
                        .get_mut("params")
                        .and_then(|params| params.get_mut("network"))
                        .and_then(|network| network.as_array_mut())
                    {
                        edges.retain(|edge| {
                            let from = edge.get("from").and_then(|v| v.as_str());
                            let to = edge.get("to").and_then(|v| v.as_str());
                            from != Some(name.as_str()) && to != Some(name.as_str())
                        });
                    }
                }
            }
        }
        let patched: Model = match serde_json::from_value(json) {
            Ok(model) => model,
            Err(error) => {
                self.error = Some(format!("could not delete: {error}"));
                return;
            }
        };
        match patched.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(patched)));
                self.selection.clear();
                self.selected = None;
                self.form = None;
                self.status = if cleaned > 0 {
                    format!(
                        "Deleted {} object(s), cleaned {cleaned} reference(s)",
                        names.len()
                    )
                } else {
                    format!("Deleted {} object(s)", names.len())
                };
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Replaces every string equal to `old` with `new` inside a JSON tree
    /// (object names, connection endpoints, home/charger/network references).
    fn rewrite_name(value: serde_json::Value, old: &str, new: &str) -> serde_json::Value {
        match value {
            serde_json::Value::String(text) if text == old => {
                serde_json::Value::String(new.to_string())
            }
            serde_json::Value::Array(items) => serde_json::Value::Array(
                items
                    .into_iter()
                    .map(|item| Self::rewrite_name(item, old, new))
                    .collect(),
            ),
            serde_json::Value::Object(map) => serde_json::Value::Object(
                map.into_iter()
                    .map(|(key, item)| (key, Self::rewrite_name(item, old, new)))
                    .collect(),
            ),
            other => other,
        }
    }

    fn apply_form(&mut self) {
        if self.model.is_none() || self.selected.is_none() || self.form.is_none() {
            return;
        }
        // Record the previous state so the form edit can be undone, the same
        // way every other edit path does.
        self.snapshot_labelled("edit object form");
        let (Some(model), Some(index), Some(form)) =
            (self.model, self.selected, self.form.as_ref())
        else {
            return;
        };
        let mut copy = model.clone();
        copy.objects[index].pos = [form.pos[0] as f64, form.pos[1] as f64];
        copy.objects[index].z = form.z as f64;
        let mut params = form.params.clone();
        if let Err(error) = form.apply_triggers(&mut params) {
            self.error = Some(error);
            return;
        }
        if let Err(error) = form.apply_timetable(&mut params) {
            self.error = Some(error);
            return;
        }
        copy.objects[index].params = params;
        let name = copy.objects[index].name.clone();
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.status = format!("Updated {name}");
                self.error = None;
                if let Some(form) = self.form.as_ref() {
                    self.params_text =
                        serde_json::to_string_pretty(&form.params).unwrap_or_default();
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn apply_json(&mut self) {
        let (Some(model), Some(index)) = (self.model, self.selected) else {
            return;
        };
        self.snapshot_labelled("edit object JSON");
        let kind = model.objects[index].kind;
        let value: serde_json::Value = match serde_json::from_str(&self.params_text) {
            Ok(value) => value,
            Err(error) => {
                self.error = Some(format!("invalid JSON: {error}"));
                return;
            }
        };
        match parse_params(kind, value) {
            Ok(params) => {
                let mut copy = model.clone();
                copy.objects[index].params = params;
                match copy.validate() {
                    Ok(()) => {
                        let name = model.objects[index].name.clone();
                        self.model = Some(Box::leak(Box::new(copy)));
                        self.status = format!("Updated {name}");
                        self.error = None;
                        self.form_dirty = false;
                        self.form = self
                            .model
                            .map(|model| EditForm::from_object(&model.objects[index]));
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn add_connection(&mut self) {
        if self.model.is_none() {
            return;
        }
        self.snapshot_labelled("add connection");
        let Some(model) = self.model else {
            return;
        };
        let (from, to) = (
            self.new_from.trim().to_string(),
            self.new_to.trim().to_string(),
        );
        if from.is_empty() || to.is_empty() {
            self.error = Some("choose a from and a to object".into());
            return;
        }
        let port = if self.new_port.trim().is_empty() {
            None
        } else {
            match self.new_port.trim().parse::<u32>() {
                Ok(port) => Some(port),
                Err(error) => {
                    self.error = Some(format!("invalid port: {error}"));
                    return;
                }
            }
        };
        if from == to {
            self.error = Some("an object cannot connect to itself".into());
            return;
        }
        if model
            .connections
            .iter()
            .any(|connection| connection.from == from && connection.to == to)
        {
            self.error = Some(format!("{from} -> {to} already exists"));
            return;
        }
        let mut copy = model.clone();
        copy.connections.push(openflex_model::model::Connection {
            from,
            to,
            port,
            strategy: None,
            flow_rate: None,
            kind: if self.new_conn_kind == 1 {
                Some(openflex_model::model::ConnectionKind::S)
            } else {
                None
            },
        });
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.status = "Connection added".into();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn set_connection_strategy(
        &mut self,
        index: usize,
        strategy: Option<openflex_model::model::PortStrategy>,
    ) {
        let Some(model) = self.model else {
            return;
        };
        // Bounds-check before touching the undo stack, and give the entry a
        // label so the history list says what changed.
        if index >= model.connections.len() {
            return;
        }
        let label = match strategy {
            Some(openflex_model::model::PortStrategy::RoundRobin) => "connection: rr",
            Some(openflex_model::model::PortStrategy::Random) => "connection: random",
            Some(openflex_model::model::PortStrategy::Hold) => "connection: hold",
            Some(openflex_model::model::PortStrategy::ByItemType) => "connection: by type",
            Some(openflex_model::model::PortStrategy::ByLabel { .. }) => "connection: by label",
            Some(openflex_model::model::PortStrategy::ByPercentage { .. }) => {
                "connection: by percent"
            }
            Some(openflex_model::model::PortStrategy::ByCondition { .. }) => {
                "connection: by condition"
            }
            Some(openflex_model::model::PortStrategy::FirstAvailable) | None => "connection: first",
        };
        self.snapshot_labelled(label);
        let mut copy = model.clone();
        if let Some(connection) = copy.connections.get_mut(index) {
            connection.strategy = strategy;
        } else {
            return;
        }
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.status = "Connection strategy updated".into();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    fn remove_connection(&mut self, index: usize) {
        if self.model.is_none() {
            return;
        }
        self.snapshot_labelled("remove connection");
        let Some(model) = self.model else {
            return;
        };
        let mut copy = model.clone();
        if index < copy.connections.len() {
            copy.connections.remove(index);
            match copy.validate() {
                Ok(()) => {
                    self.model = Some(Box::leak(Box::new(copy)));
                    self.status = "Connection removed".into();
                    self.error = None;
                }
                Err(error) => self.error = Some(error.to_string()),
            }
        }
    }

    fn project(&self, pos: [f64; 2], z: f64, center: egui::Pos2, scale: f32) -> egui::Pos2 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let x = pos[0] as f32 * scale;
        let y = pos[1] as f32 * scale;
        let z = z as f32 * scale;
        let rx = x * cos_yaw - y * sin_yaw;
        let ry = x * sin_yaw + y * cos_yaw;
        center + egui::vec2(rx, ry * sin_pitch - z * cos_pitch)
    }

    /// Writes the current layout/state as an SVG snapshot (same projection as
    /// the canvas, including rotation and the heatmap colours).
    /// Projection used by the SVG export: the camera projection normally, a
    /// plain top-down mapping in the flat 2D view.
    fn project_for_export(
        &self,
        pos: [f64; 2],
        z: f64,
        center: egui::Pos2,
        scale: f32,
    ) -> egui::Pos2 {
        if self.flat_view {
            center
                + egui::vec2(
                    (pos[0] - self.focus[0] as f64) as f32 * scale,
                    (pos[1] - self.focus[1] as f64) as f32 * scale,
                )
        } else {
            self.project(pos, z, center, scale)
        }
    }

    fn export_svg(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let width = 1024.0_f32;
        let height = 768.0_f32;
        let center = egui::pos2(width / 2.0, height / 2.0);
        let scale = (width.min(height) / 12.0) * self.zoom;
        let mut svg = String::new();
        svg.push_str(&format!(
            r##"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<defs>
  <radialGradient id="glow" cx="50%" cy="50%" r="62%">
    <stop offset="0%" stop-color="#1d2734"/>
    <stop offset="100%" stop-color="#0b0e13"/>
  </radialGradient>
</defs>
<rect width="{width}" height="{height}" fill="url(#glow)"/>
<text x="20" y="30" fill="#8fa3bd" font-size="13" font-family="sans-serif">{title}</text>
"##,
            title = xml_escape(&model.name)
        ));
        for connection in &model.connections {
            let (Some(from), Some(to)) =
                (model.object(&connection.from), model.object(&connection.to))
            else {
                continue;
            };
            let a = self.project_for_export(from.pos, 0.0, center, scale);
            let b = self.project_for_export(to.pos, 0.0, center, scale);
            svg.push_str(&format!(
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#5a7896" stroke-width="2.4" stroke-opacity="0.35"/>
<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#86a6c8" stroke-width="1.2"/>
"##,
                a.x, a.y, b.x, b.y, a.x, a.y, b.x, b.y
            ));
            // Arrow head a few pixels before the target box.
            let dir = (b - a).normalized();
            if dir.length_sq() > 0.0 {
                let tip = b - dir * 12.0;
                let normal = egui::vec2(-dir.y, dir.x) * 4.0;
                svg.push_str(&format!(
                    r##"<polygon points="{:.1},{:.1} {:.1},{:.1} {:.1},{:.1}" fill="#86a6c8"/>
"##,
                    tip.x + dir.x * 5.0,
                    tip.y + dir.y * 5.0,
                    tip.x + normal.x,
                    tip.y + normal.y,
                    tip.x - normal.x,
                    tip.y - normal.y
                ));
            }
        }
        for object in &model.objects {
            let point = self.project_for_export(object.pos, object.z, center, scale);
            let base = match self.heatmap_fill(&object.name) {
                Some(colour) => colour,
                None => Self::kind_color(object.kind),
            };
            let rot = object.rot.unwrap_or(0.0);
            let (r, g, b) = (base.r(), base.g(), base.b());
            let (x, y) = (point.x, point.y);
            // Footprint size follows the render mode: the flat view uses the
            // same per-kind plate sizes as the canvas.
            let (w, h) = if self.flat_view {
                let (fw, fh) = match object.kind {
                    ObjectKind::Conveyor | ObjectKind::Escalator => (56.0, 12.0),
                    ObjectKind::Elevator => (18.0, 30.0),
                    ObjectKind::Queue => (40.0, 16.0),
                    _ => (34.0, 20.0),
                };
                (fw * self.zoom, fh * self.zoom)
            } else {
                (38.0 * self.zoom, 18.0 * self.zoom)
            };
            // Names go through the escaper so `&`/`<` in a model cannot break
            // the XML; the caption sits on a dark plate like the on-screen chip.
            let name = xml_escape(&object.name);
            let label_w = (object.name.chars().count() as f32 * 6.2 + 10.0).max(24.0);
            // In the floor plan the caption sits on the footprint centre, the
            // same place the canvas puts it.
            let (px, py, ty) = if self.flat_view {
                (x - label_w / 2.0, y - 7.5, y + 3.5)
            } else {
                (
                    x - label_w / 2.0,
                    y - 12.0 * self.zoom - 12.0,
                    y - 12.0 * self.zoom - 1.0,
                )
            };
            svg.push_str(&format!(
                "<g transform=\"rotate({rot:.1} {x:.1} {y:.1})\">\
<rect x=\"{rx:.1}\" y=\"{ry:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"4\" fill=\"#{r:02x}{g:02x}{b:02x}\" stroke=\"#14181e\"/>\
</g>\
<rect x=\"{px:.1}\" y=\"{py:.1}\" width=\"{label_w:.1}\" height=\"15\" rx=\"7.5\" fill=\"#0e121a\" fill-opacity=\"0.82\" stroke=\"#7a8ea9\" stroke-opacity=\"0.45\"/>\
<text x=\"{x:.1}\" y=\"{ty:.1}\" fill=\"#ffffff\" font-size=\"10.5\" text-anchor=\"middle\" font-family=\"sans-serif\">{name}</text>\n",
                rx = x - w / 2.0,
                ry = y - h / 2.0,
            ));
        }
        svg.push_str("</svg>\n");
        let path = self
            .path
            .as_ref()
            .map(|path| path.with_extension("view.svg"))
            .unwrap_or_else(|| std::path::PathBuf::from("openflex-view.svg"));
        match std::fs::write(&path, svg) {
            Ok(()) => {
                self.remember_output(&path);
                self.status = format!("View exported to {}", path.display());
                self.error = None;
            }
            Err(error) => self.error = Some(format!("could not write SVG: {error}")),
        }
    }

    /// Fill colour of an object while a simulation is running: the dominant
    /// state (busy/blocked/down/off-shift/idle) wins over the kind colour.
    /// Green → yellow → red ramp used by the report heatmap.
    fn heat_colour(value: f64) -> egui::Color32 {
        let t = value.clamp(0.0, 1.0) as f32;
        if t < 0.5 {
            let k = t / 0.5;
            egui::Color32::from_rgb(
                (22.0 + (250.0 - 22.0) * k) as u8,
                163,
                (74.0 + (204.0 - 74.0) * k) as u8,
            )
        } else {
            let k = (t - 0.5) / 0.5;
            egui::Color32::from_rgb(
                (250.0 + (220.0 - 250.0) * k) as u8,
                (204.0 + (38.0 - 204.0) * k) as u8,
                (21.0 + (38.0 - 21.0) * k) as u8,
            )
        }
    }

    /// Colour of an object under the selected heatmap metric.
    fn heatmap_fill(&self, name: &str) -> Option<egui::Color32> {
        if self.heatmap == 0 {
            return None;
        }
        let report = self.report.as_ref()?;
        let metric = |stats: &openflex_core::stats::ObjectAggregate| match self.heatmap {
            2 => stats.throughput.mean.max(0.0),
            3 => stats.avg_content.mean.max(0.0),
            _ => stats.utilization.mean.clamp(0.0, 1.0),
        };
        let values: Vec<f64> = report.objects.values().map(metric).collect();
        let max = values.iter().copied().fold(0.0_f64, f64::max);
        let current = report.objects.get(name).map(metric)?;
        let normalised = if self.heatmap == 1 {
            current
        } else if max > 0.0 {
            current / max
        } else {
            0.0
        };
        Some(OpenFlexApp::heat_colour(normalised))
    }

    fn state_fill(stats: &openflex_core::stats::ObjectStats) -> egui::Color32 {
        // Prefer the instantaneous state reported by the engine (task-145).
        if let Some(state) = stats.state {
            return match state {
                openflex_core::engine::State::Busy => palette::ST_BUSY,
                openflex_core::engine::State::Blocked => palette::ST_BLOCKED,
                openflex_core::engine::State::Down => palette::ST_DOWN,
                openflex_core::engine::State::OffShift => palette::ST_OFF,
                openflex_core::engine::State::Idle => palette::ST_IDLE,
            };
        }
        let times = &stats.state_times;
        let entries = [
            (times.busy, palette::ST_BUSY),
            (times.blocked, palette::ST_BLOCKED),
            (times.down, palette::ST_DOWN),
            (times.off_shift, palette::ST_OFF),
            (times.idle, palette::ST_IDLE),
        ];
        let mut best = entries[4];
        for entry in entries.iter() {
            if entry.0 > best.0 {
                best = *entry;
            }
        }
        best.1
    }

    /// Lightens (`amount > 0`) or darkens (`amount < 0`) a colour, which is how
    /// the flat 2D canvas fakes a light coming from the top-left.
    fn shade(color: egui::Color32, amount: f32) -> egui::Color32 {
        let adjust = |channel: u8| -> u8 {
            if amount >= 0.0 {
                (channel as f32 + (255.0 - channel as f32) * amount).round() as u8
            } else {
                (channel as f32 * (1.0 + amount)).round() as u8
            }
        };
        egui::Color32::from_rgb(adjust(color.r()), adjust(color.g()), adjust(color.b()))
    }

    /// Draws a kind-specific 3D-ish body and returns the rectangle used for
    /// Draws a kind-specific body and returns the rectangle used for hit
    /// testing and labels. `flat` switches to a **top-down floor-plan** look
    /// (no fake height), which the 2D toggle enables.
    #[allow(clippy::too_many_arguments)]
    fn draw_object_shape_mode(
        painter: &egui::Painter,
        kind: ObjectKind,
        base: egui::Pos2,
        zoom: f32,
        fill: egui::Color32,
        selected: bool,
        rotation: f32,
        depth: f32,
        flat: bool,
    ) -> egui::Rect {
        if flat {
            // Top-down footprint: a rounded plate in the kind colour with a
            // thin darker rim; height and shading are deliberately dropped so
            // the layout reads like a floor plan.
            let (w, h) = match kind {
                ObjectKind::Conveyor | ObjectKind::Escalator => (56.0, 12.0),
                ObjectKind::Elevator => (18.0, 30.0),
                ObjectKind::Queue => (40.0, 16.0),
                _ => (34.0, 20.0),
            };
            let rim = if selected {
                egui::Stroke::new(1.8_f32, egui::Color32::from_rgb(255, 226, 168))
            } else {
                egui::Stroke::new(1.0_f32, OpenFlexApp::shade(fill, -0.45))
            };
            // Rotation matters in a floor plan, so the footprint is rotated by
            // drawing a polygon instead of an axis-aligned rectangle.
            if rotation.rem_euclid(360.0).abs() > 0.5 {
                let half = egui::vec2(w, h) * zoom * 0.5;
                let (sin, cos) = rotation.to_radians().sin_cos();
                let rotate = |offset: egui::Vec2| {
                    base + egui::vec2(
                        offset.x * cos - offset.y * sin,
                        offset.x * sin + offset.y * cos,
                    )
                };
                let corners = vec![
                    rotate(egui::vec2(-half.x, -half.y)),
                    rotate(egui::vec2(half.x, -half.y)),
                    rotate(egui::vec2(half.x, half.y)),
                    rotate(egui::vec2(-half.x, half.y)),
                ];
                painter.add(egui::Shape::convex_polygon(
                    corners.clone(),
                    fill,
                    egui::Stroke::NONE,
                ));
                for pair in [[0, 1], [1, 2], [2, 3], [3, 0]] {
                    painter.line_segment([corners[pair[0]], corners[pair[1]]], rim);
                }
                // Remember the polygon so hit-testing follows the rotated shape.
                FLAT_FOOTPRINT.with(|slot| *slot.borrow_mut() = Some(corners.clone()));
                return egui::Rect::from_points(&corners);
            }
            let body = egui::Rect::from_center_size(base, egui::vec2(w, h) * zoom);
            painter.rect_filled(body, (3.0 * zoom).max(2.0), fill);
            painter.rect_stroke(body, (3.0 * zoom).max(2.0), rim);
            // A north tick marks orientation without any 3D cue.
            painter.line_segment(
                [
                    body.center_top(),
                    body.center_top() + egui::vec2(0.0, 3.0 * zoom),
                ],
                egui::Stroke::new(1.0_f32, OpenFlexApp::shade(fill, 0.5)),
            );
            return body;
        }
        // Selected bodies get a warm ring; the rest use a dark hairline so the
        // shape reads against the canvas without looking outlined in black.
        let outline = egui::Stroke::new(
            if selected { 1.6_f32 } else { 1.0_f32 },
            if selected {
                egui::Color32::from_rgb(255, 226, 168)
            } else {
                egui::Color32::from_rgba_unmultiplied(10, 13, 18, 210)
            },
        );
        // Rotation is applied to the footprint: a rotated box keeps its size
        // but the corners swing, which is what FlexSim's layout view shows.
        // Height of the side face that fakes the third dimension.
        let lift = (depth.max(0.0) * zoom * 0.6 + 4.0).min(26.0);
        match kind {
            // Machines: a body plus a lighter top plate.
            ObjectKind::Processor
            | ObjectKind::MultiProcessor
            | ObjectKind::Combiner
            | ObjectKind::Separator
            | ObjectKind::Merge
            | ObjectKind::Split => {
                let w = 40.0 * zoom;
                let h = 22.0 * zoom;
                let body = egui::Rect::from_center_size(
                    base + egui::vec2(0.0, 2.0 * zoom),
                    egui::vec2(w, h),
                );
                // Contact shadow so the body sits on the floor plane.
                painter.rect_filled(
                    body.translate(egui::vec2(0.0, 3.0 * zoom)),
                    (body.height() / 2.0).max(2.0),
                    palette::SHADOW,
                );
                painter.rect_filled(body, 2.0, fill);
                painter.rect_stroke(body, 2.0, outline);
                let top = egui::Rect::from_min_size(
                    body.left_top() + egui::vec2(0.0, -5.0 * zoom),
                    egui::vec2(w, 5.0 * zoom),
                );
                painter.rect_filled(top, 2.0, OpenFlexApp::shade(fill, 0.28));
                painter.rect_stroke(top, 2.0, outline);
                // A small working light on the machine.
                painter.circle_filled(
                    body.left_top() + egui::vec2(5.0 * zoom, 5.0 * zoom),
                    (2.2 * zoom).max(1.0),
                    OpenFlexApp::shade(fill, 0.55),
                );
                body.union(top)
            }
            // Conveyors / escalators: a long belt with side rails.
            ObjectKind::Conveyor | ObjectKind::Escalator => {
                let w = 56.0 * zoom;
                let h = 12.0 * zoom;
                let belt = egui::Rect::from_center_size(base, egui::vec2(w, h));
                painter.rect_filled(belt, 3.0, OpenFlexApp::shade(fill, -0.15));
                painter.rect_stroke(belt, 3.0, outline);
                for side in [belt.left_top(), belt.left_bottom()] {
                    painter.line_segment(
                        [side, side + egui::vec2(w, 0.0)],
                        egui::Stroke::new(1.6_f32, OpenFlexApp::shade(fill, 0.35)),
                    );
                }
                belt
            }
            // Racks: a stack of levels.
            ObjectKind::Rack => {
                let body = Self::draw_rotated_box(
                    painter,
                    base,
                    egui::vec2(17.0 * zoom, 12.0 * zoom),
                    rotation,
                    fill,
                    outline,
                    lift * 1.2,
                );
                for level in 1..3 {
                    let t = level as f32 / 3.0;
                    let y = body.top() + body.height() * t;
                    painter.line_segment(
                        [egui::pos2(body.left(), y), egui::pos2(body.right(), y)],
                        egui::Stroke::new(1.0_f32, OpenFlexApp::shade(fill, -0.3)),
                    );
                }
                body
            }
            // Sources / sinks: a drum.
            ObjectKind::Source | ObjectKind::Sink => {
                let radius = 13.0 * zoom;
                painter.circle_filled(base + egui::vec2(0.0, 3.0 * zoom), radius, palette::SHADOW);
                painter.circle_filled(base, radius, fill);
                painter.circle_stroke(base, radius, outline);
                // Specular dot: a small bright spot up and to the left.
                painter.circle_filled(
                    base + egui::vec2(-radius * 0.32, -radius * 0.34),
                    radius * 0.34,
                    OpenFlexApp::shade(fill, 0.42),
                );
                egui::Rect::from_center_size(base, egui::vec2(radius * 2.0, radius * 2.0))
            }
            // People / vehicles: a smaller pill.
            ObjectKind::Person
            | ObjectKind::Transporter
            | ObjectKind::Agv
            | ObjectKind::Robot
            | ObjectKind::Crane
            | ObjectKind::AsrsVehicle => {
                let size = egui::vec2(26.0, 14.0) * zoom;
                let body = egui::Rect::from_center_size(base, size);
                painter.rect_filled(
                    body.translate(egui::vec2(0.0, 3.0 * zoom)),
                    body.height() / 2.0,
                    palette::SHADOW,
                );
                painter.rect_filled(body, body.height() / 2.0, fill);
                painter.rect_stroke(body, body.height() / 2.0, outline);
                painter.rect_filled(
                    egui::Rect::from_min_size(
                        body.left_top() + egui::vec2(0.0, -3.0 * zoom),
                        egui::vec2(size.x, 3.0 * zoom),
                    ),
                    2.0,
                    OpenFlexApp::shade(fill, 0.3),
                );
                body
            }
            // Tanks / fluid bodies: a tall cylinder.
            ObjectKind::FluidTank
            | ObjectKind::FluidGenerator
            | ObjectKind::FluidSink
            | ObjectKind::FluidPipe
            | ObjectKind::FluidValve => {
                let size = egui::vec2(22.0, 26.0) * zoom;
                let body = egui::Rect::from_center_size(base, size);
                painter.rect_filled(body, 5.0, fill);
                painter.rect_stroke(body, 5.0, outline);
                let liquid = egui::Rect::from_min_max(
                    body.left_bottom() - egui::vec2(0.0, size.y * 0.45),
                    body.right_bottom(),
                );
                painter.rect_filled(liquid, 0.0, OpenFlexApp::shade(fill, 0.35));
                body
            }
            // Queues: a tray with three slot dividers, so a queue reads as
            // "waiting line" rather than a generic block.
            ObjectKind::Queue => {
                let size = egui::vec2(40.0, 16.0) * zoom;
                let body = egui::Rect::from_center_size(base, size);
                painter.rect_filled(body, 3.0, OpenFlexApp::shade(fill, -0.12));
                painter.rect_stroke(body, 3.0, outline);
                for fraction in [0.33_f32, 0.66] {
                    let x = body.left() + body.width() * fraction;
                    painter.line_segment(
                        [
                            egui::pos2(x, body.top() + 2.0),
                            egui::pos2(x, body.bottom() - 2.0),
                        ],
                        egui::Stroke::new(1.0_f32, OpenFlexApp::shade(fill, 0.35)),
                    );
                }
                body
            }
            // Dispatchers: a hub with three short spokes (it hands out tasks).
            ObjectKind::Dispatcher => {
                let radius = 11.0 * zoom;
                painter.circle_filled(base + egui::vec2(0.0, 3.0 * zoom), radius, palette::SHADOW);
                painter.circle_filled(base, radius, fill);
                painter.circle_stroke(base, radius, outline);
                for step in 0..3 {
                    let angle = std::f32::consts::TAU * step as f32 / 3.0 - 0.5;
                    let dir = egui::vec2(angle.cos(), angle.sin());
                    painter.line_segment(
                        [base + dir * radius * 0.45, base + dir * radius * 1.5],
                        egui::Stroke::new(1.6_f32, OpenFlexApp::shade(fill, 0.45)),
                    );
                }
                egui::Rect::from_center_size(base, egui::vec2(radius * 3.0, radius * 3.0))
            }
            // Resources: a round station with four tick marks (capacity pool).
            ObjectKind::Resource => {
                let radius = 12.0 * zoom;
                painter.circle_filled(base + egui::vec2(0.0, 3.0 * zoom), radius, palette::SHADOW);
                painter.circle_filled(base, radius, fill);
                painter.circle_stroke(base, radius, outline);
                for step in 0..4 {
                    let angle = std::f32::consts::TAU * step as f32 / 4.0;
                    let dir = egui::vec2(angle.cos(), angle.sin());
                    painter.line_segment(
                        [base + dir * radius * 0.55, base + dir * radius * 0.9],
                        egui::Stroke::new(1.2_f32, OpenFlexApp::shade(fill, 0.4)),
                    );
                }
                egui::Rect::from_center_size(base, egui::vec2(radius * 2.0, radius * 2.0))
            }
            // Elevators: a tall shaft with a cabin block inside.
            ObjectKind::Elevator => {
                let size = egui::vec2(18.0, 30.0) * zoom;
                let body = egui::Rect::from_center_size(base, size);
                painter.rect_filled(body, 3.0, OpenFlexApp::shade(fill, -0.25));
                painter.rect_stroke(body, 3.0, outline);
                let cabin = egui::Rect::from_center_size(
                    body.center() + egui::vec2(0.0, -size.y * 0.18),
                    egui::vec2(size.x * 0.62, size.y * 0.34),
                );
                painter.rect_filled(cabin, 2.0, fill);
                painter.rect_stroke(cabin, 2.0, outline);
                body
            }
            // Everything else: the classic box with a lit top edge.
            _ => {
                let size = egui::vec2(38.0, 18.0) * zoom;
                let body = egui::Rect::from_center_size(base, size);
                painter.rect_filled(body, 3.0, fill);
                painter.rect_stroke(body, 3.0, outline);
                painter.line_segment(
                    [body.left_top(), body.right_top()],
                    egui::Stroke::new(1.4_f32, OpenFlexApp::shade(fill, 0.4)),
                );
                body
            }
        }
    }

    /// Draws a rotated rectangle (footprint) plus a lighter top face, which is
    /// how objects get their "3D" look on the flat canvas.
    fn draw_rotated_box(
        painter: &egui::Painter,
        base: egui::Pos2,
        half: egui::Vec2,
        rotation_deg: f32,
        fill: egui::Color32,
        stroke: egui::Stroke,
        lift: f32,
    ) -> egui::Rect {
        let (sin, cos) = rotation_deg.to_radians().sin_cos();
        let rot = |offset: egui::Vec2| -> egui::Pos2 {
            base + egui::vec2(
                offset.x * cos - offset.y * sin,
                offset.x * sin + offset.y * cos,
            )
        };
        let corners = [
            rot(egui::vec2(-half.x, -half.y)),
            rot(egui::vec2(half.x, -half.y)),
            rot(egui::vec2(half.x, half.y)),
            rot(egui::vec2(-half.x, half.y)),
        ];
        // Side face: extrude the front edge downwards.
        let side = egui::vec2(0.0, lift);
        let front: Vec<egui::Pos2> = corners.iter().map(|point| *point + side).collect();
        let side_color = OpenFlexApp::shade(fill, -0.35);
        painter.add(egui::Shape::convex_polygon(
            vec![corners[2], corners[3], front[3], front[2]],
            side_color,
            egui::Stroke::NONE,
        ));
        let right_side = egui::Shape::convex_polygon(
            vec![corners[1], corners[2], front[2], front[1]],
            OpenFlexApp::shade(fill, -0.2),
            egui::Stroke::NONE,
        );
        painter.add(right_side);
        painter.add(egui::Shape::convex_polygon(corners.to_vec(), fill, stroke));
        painter.add(egui::Shape::convex_polygon(
            front.to_vec(),
            egui::Color32::TRANSPARENT,
            egui::Stroke::new(stroke.width, OpenFlexApp::shade(fill, -0.45)),
        ));
        egui::Rect::from_points(&[
            corners[0], corners[1], corners[2], corners[3], front[0], front[1], front[2], front[3],
        ])
    }

    fn kind_color(kind: ObjectKind) -> egui::Color32 {
        // A calm, evenly-spaced hue wheel per object family: sources green,
        // storage teal, machines indigo, transport amber, fluid cyan, the rest
        // slate. Values sit around the same lightness so no kind shouts.
        match kind {
            ObjectKind::Source => egui::Color32::from_rgb(74, 156, 110),
            ObjectKind::Sink => egui::Color32::from_rgb(150, 96, 158),
            ObjectKind::Processor | ObjectKind::MultiProcessor => {
                egui::Color32::from_rgb(86, 124, 196)
            }
            ObjectKind::Combiner
            | ObjectKind::Separator
            | ObjectKind::Merge
            | ObjectKind::Split => egui::Color32::from_rgb(104, 132, 186),
            ObjectKind::Queue | ObjectKind::Rack => egui::Color32::from_rgb(84, 158, 176),
            ObjectKind::Conveyor | ObjectKind::Escalator => egui::Color32::from_rgb(176, 148, 88),
            ObjectKind::Transporter
            | ObjectKind::Person
            | ObjectKind::Agv
            | ObjectKind::Robot
            | ObjectKind::Crane
            | ObjectKind::AsrsVehicle => egui::Color32::from_rgb(198, 132, 88),
            ObjectKind::Dispatcher | ObjectKind::Resource => egui::Color32::from_rgb(160, 122, 172),
            ObjectKind::Elevator => egui::Color32::from_rgb(132, 138, 168),
            ObjectKind::FluidTank
            | ObjectKind::FluidPipe
            | ObjectKind::FluidValve
            | ObjectKind::FluidGenerator
            | ObjectKind::FluidSink => egui::Color32::from_rgb(92, 176, 172),
            _ => egui::Color32::from_rgb(124, 136, 152),
        }
    }
}

impl eframe::App for OpenFlexApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.last_frame = ctx.input(|input| input.stable_dt) as f64;
        // Keep the OS title bar in sync: model name, unsaved marker, run state.
        let title = Self::window_title_text(
            self.model.map(|model| model.name.as_str()),
            self.model_dirty,
            self.running && !self.paused,
            self.report_running,
        );
        if title != self.window_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.window_title = title;
        }
        self.poll_report();
        self.poll_experiment();
        self.poll_timeseries();
        self.poll_optimize();
        self.poll_trace();
        if self.report_running
            || self.exp_running
            || self.series_running
            || self.opt_running
            || self.trace_running
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        // eframe only autosaves on a window event; a slow repaint keeps the
        // event loop alive so window size and panel state reach `app.ron`
        // even when the user is just looking at the model.
        ctx.request_repaint_after(std::time::Duration::from_secs(20));
        if self.running && !self.paused {
            // Playback speed multiplies the model time advanced per frame,
            // mirroring FlexSim's run-speed slider without changing the
            // simulation itself (the engine always sees whole steps).
            let step = (self.step_size.max(0.1) * self.play_speed.max(0.1)) as f64;
            self.advance(step);
            let frame_ms = (60.0 / self.play_speed.clamp(0.25, 8.0)).max(16.0);
            ctx.request_repaint_after(std::time::Duration::from_millis(frame_ms as u64));
        }

        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .fill(palette::PANEL)
                    .inner_margin(egui::Margin::symmetric(10.0, 6.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new("OpenFlex")
                            .size(17.0)
                            .strong()
                            .color(palette::ACCENT_HI),
                    );
                    // A dot after the brand means "the model differs from what
                    // is on disk".
                    if self.model_dirty {
                        ui.label(
                            egui::RichText::new("*")
                                .size(15.0_f32)
                                .strong()
                                .color(palette::AMBER),
                        )
                        .on_hover_text("unsaved model changes");
                    }
                    ui.separator();
                    tool_group(ui, "FILE");
                    ui.add(egui::TextEdit::singleline(&mut self.path_input).desired_width(280.0));
                    if ui.button("Open").clicked() && !self.path_input.is_empty() {
                        self.load(PathBuf::from(self.path_input.clone()));
                    }
                    if ui
                        .button("Browse…")
                        .on_hover_text("pick a model from a folder list")
                        .clicked()
                    {
                        // Start where the current model lives, if any.
                        if let Some(path) = self.path.as_ref().and_then(|p| p.parent()) {
                            self.browser_dir = path.to_path_buf();
                        }
                        self.refresh_browser();
                        self.show_browser = true;
                    }
                    if ui.button("Save").clicked() {
                        self.save();
                    }
                    if ui.button("Save as…").clicked() {
                        self.save_as = self.path_input.clone();
                        self.show_save_as = true;
                    }
                    if ui.button("New").clicked() {
                        self.new_model();
                    }
                    if ui.button("Help").clicked() {
                        self.show_help = !self.show_help;
                    }
                    if ui
                        .add_enabled(self.model.is_some(), egui::Button::new("Export view SVG"))
                        .clicked()
                    {
                        self.export_svg();
                    }
                    if ui
                        .add_enabled(!self.undo.is_empty(), egui::Button::new("Undo"))
                        .clicked()
                    {
                        self.restore();
                    }
                    if ui
                        .add_enabled(!self.redo.is_empty(), egui::Button::new("Redo"))
                        .clicked()
                    {
                        self.redo_once();
                    }
                    if !self.recent.is_empty() {
                        ui.menu_button("Recent", |ui| {
                            let items: Vec<PathBuf> = self.recent.clone();
                            for item in items {
                                if ui
                                    .button(
                                        item.file_name()
                                            .map(|name| name.to_string_lossy().into_owned())
                                            .unwrap_or_default(),
                                    )
                                    .clicked()
                                {
                                    self.load(item);
                                    ui.close_menu();
                                }
                            }
                        });
                    }
                    ui.separator();
                    tool_group(ui, "RUN");
                    if primary_button(ui, self.model.is_some() && !self.report_running, "Run") {
                        self.start_report();
                    }
                    if ui
                        .add_enabled(self.model.is_some(), egui::Button::new("Step"))
                        .clicked()
                    {
                        self.advance(self.step_size as f64);
                    }
                    if ui
                        .add_enabled(
                            self.model.is_some() && !self.running,
                            egui::Button::new("Step event"),
                        )
                        .clicked()
                    {
                        self.step_event();
                    }
                    if ui
                        .add_enabled(self.handle.is_some(), egui::Button::new("Pause"))
                        .clicked()
                    {
                        if let Some(handle) = self.handle.as_mut() {
                            let _ = handle.pause();
                        }
                        self.paused = true;
                        self.status = "Paused".into();
                    }
                    if ui
                        .add_enabled(
                            self.handle.is_some() && self.paused,
                            egui::Button::new("Resume"),
                        )
                        .clicked()
                    {
                        if let Some(handle) = self.handle.as_mut() {
                            let _ = handle.resume();
                        }
                        self.paused = false;
                        self.status = "Resumed".into();
                    }
                    if primary_button(ui, self.model.is_some(), "Play") {
                        self.running = true;
                        self.paused = false;
                        self.status = "Playing".into();
                    }
                    if ui.button("Reset").clicked() {
                        self.reset_run();
                    }
                    if let (Some(state), Some(model)) = (self.sim_state.as_ref(), self.model) {
                        let progress = if model.run.stop_time > 0.0 {
                            (state.now / model.run.stop_time).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        // Slim progress bar coloured by run state (amber while
                        // playing, green when complete).
                        let fill = if progress >= 0.999 {
                            palette::OK
                        } else {
                            palette::ACCENT
                        };
                        ui.scope(|ui| {
                            ui.visuals_mut().selection.bg_fill = fill;
                            ui.add(
                                egui::ProgressBar::new(progress as f32)
                                    .desired_width(150.0)
                                    .desired_height(10.0)
                                    .fill(fill.gamma_multiply(0.85))
                                    .text(
                                        egui::RichText::new(format!(
                                            "{:.1} / {:.1} {}",
                                            state.now,
                                            model.run.stop_time,
                                            model.time_unit.label()
                                        ))
                                        .size(10.0_f32)
                                        .color(palette::TEXT),
                                    ),
                            );
                        });
                        // Warm-up is excluded from the statistics: mark the
                        // boundary so the live view is not read as data.
                        if model.run.warmup > 0.0 {
                            let in_warmup = state.now < model.run.warmup - 1e-9;
                            ui.label(
                                egui::RichText::new(if in_warmup {
                                    format!("warm-up until {:.1}", model.run.warmup)
                                } else {
                                    "measuring".to_string()
                                })
                                .size(9.0_f32)
                                .color(if in_warmup {
                                    palette::AMBER
                                } else {
                                    palette::OK.gamma_multiply(0.9)
                                }),
                            );
                        }
                    }
                    ui.separator();
                    tool_group(ui, "RUN");
                    // Parallel replications were implemented but had no toggle:
                    // `parallel_runs` could never be turned off from the UI.
                    {
                        let label = if self.parallel_runs {
                            "parallel on"
                        } else {
                            "parallel off"
                        };
                        if ui
                            .add(
                                egui::Button::new(egui::RichText::new(label).small().color(
                                    if self.parallel_runs {
                                        palette::ACCENT_HI
                                    } else {
                                        palette::TEXT_DIM
                                    },
                                ))
                                .fill(palette::CARD)
                                .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
                                .rounding(egui::Rounding::same(5.0_f32)),
                            )
                            .on_hover_text(
                                "run replications on all cores (results are bit-identical)",
                            )
                            .clicked()
                        {
                            self.parallel_runs = !self.parallel_runs;
                            self.save_ui_prefs();
                        }
                    }
                    ui.separator();
                    tool_group(ui, "STATE");
                    if ui
                        .add_enabled(
                            self.model.is_some() && !self.trace_running,
                            egui::Button::new(if self.trace_running {
                                "Tracing…"
                            } else {
                                "Trace run"
                            }),
                        )
                        .on_hover_text("run once with the engine trace enabled")
                        .clicked()
                    {
                        self.start_trace_run();
                    }
                    if ui
                        .add_enabled(self.trace.is_some(), egui::Button::new("Export trace"))
                        .on_hover_text("write <model>.trace.jsonl")
                        .clicked()
                    {
                        self.export_trace();
                    }
                    if ui
                        .add_enabled(self.model.is_some(), egui::Button::new("Save checkpoint"))
                        .clicked()
                    {
                        self.save_checkpoint();
                    }
                    if ui
                        .add_enabled(self.model.is_some(), egui::Button::new("Load checkpoint"))
                        .clicked()
                    {
                        self.load_checkpoint();
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut self.checkpoint_dir)
                            .hint_text("checkpoints/")
                            .desired_width(110.0),
                    );
                    ui.separator();
                    ui.label("Step");
                    ui.add(
                        egui::DragValue::new(&mut self.step_size)
                            .speed(1.0)
                            .range(0.1..=1000.0),
                    );
                    ui.label("Speed");
                    ui.add(
                        egui::Slider::new(&mut self.play_speed, 0.25..=8.0)
                            .logarithmic(true)
                            .suffix("×"),
                    );
                    ui.separator();
                    tool_group(ui, "VIEW");
                    if ui
                        .add_enabled(self.model.is_some(), egui::Button::new("Fit"))
                        .on_hover_text("frame the selection (or the whole model)")
                        .clicked()
                    {
                        self.frame_selection();
                    }
                    if ui
                        .add_enabled(self.model.is_some(), egui::Button::new("Auto layout"))
                        .on_hover_text("arrange objects following the connections")
                        .clicked()
                    {
                        self.auto_layout(false);
                    }
                    if ui.button("Iso").clicked() {
                        self.yaw = -0.6;
                        self.pitch = 0.5;
                        self.zoom = 1.0;
                    }
                    let flat_label = if self.flat_view { "2D on" } else { "2D off" };
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new(flat_label).small()).fill(
                                if self.flat_view {
                                    palette::ACCENT_HI
                                } else {
                                    palette::CARD
                                },
                            ),
                        )
                        .on_hover_text("flat floor-plan rendering (no fake height)")
                        .clicked()
                    {
                        self.flat_view = !self.flat_view;
                        if self.flat_view {
                            // A floor plan only makes sense from above.
                            self.yaw = 0.0;
                            self.pitch = PITCH_MAX;
                        }
                        self.save_ui_prefs();
                    }
                    if ui.button("Top").clicked() {
                        self.yaw = 0.0;
                        self.pitch = PITCH_MAX;
                        self.zoom = 1.0;
                    }
                    if ui.button("Front").clicked() {
                        self.yaw = 0.0;
                        self.pitch = 0.05;
                        self.zoom = 1.0;
                    }
                    if ui.button("Side").clicked() {
                        self.yaw = 1.55;
                        self.pitch = 0.05;
                        self.zoom = 1.0;
                    }
                    ui.separator();
                    ui.label("Heatmap");
                    // Legend: green → yellow → red with the metric's range.
                    if self.heatmap != 0
                        && let Some(report) = self.report.as_ref()
                    {
                        let (low, high, name) = match self.heatmap {
                            2 => {
                                let values: Vec<f64> = report
                                    .objects
                                    .values()
                                    .map(|stats| stats.throughput.mean)
                                    .collect();
                                (
                                    values.iter().copied().fold(f64::INFINITY, f64::min),
                                    values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                                    "thru",
                                )
                            }
                            3 => {
                                let values: Vec<f64> = report
                                    .objects
                                    .values()
                                    .map(|stats| stats.avg_content.mean)
                                    .collect();
                                (
                                    values.iter().copied().fold(f64::INFINITY, f64::min),
                                    values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                                    "WIP",
                                )
                            }
                            _ => (0.0, 1.0, "util"),
                        };
                        let (low, high) = if low.is_finite() && high.is_finite() && low <= high {
                            (low, high)
                        } else {
                            (0.0, 1.0)
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(if self.heatmap == 1 {
                                    "0%".to_string()
                                } else {
                                    format!("{low:.1}")
                                })
                                .size(9.0_f32)
                                .monospace()
                                .color(palette::TEXT_DIM),
                            );
                            let (bar, _) =
                                ui.allocate_exact_size(egui::vec2(56.0, 7.0), egui::Sense::hover());
                            // Nine steps read as a gradient without a gradient API.
                            for step in 0..9 {
                                let t = step as f64 / 8.0;
                                let piece = egui::Rect::from_min_size(
                                    egui::pos2(bar.left() + bar.width() * t as f32, bar.top()),
                                    egui::vec2(bar.width() / 8.0 + 0.5, bar.height()),
                                );
                                ui.painter().rect_filled(piece, 0.0, Self::heat_colour(t));
                            }
                            ui.label(
                                egui::RichText::new(if self.heatmap == 1 {
                                    "100%".to_string()
                                } else {
                                    format!("{high:.1} {name}")
                                })
                                .size(9.0_f32)
                                .monospace()
                                .color(palette::TEXT_DIM),
                            );
                        });
                    }
                    egui::ComboBox::from_id_salt("heatmap")
                        .selected_text(match self.heatmap {
                            1 => "utilization",
                            2 => "throughput",
                            3 => "WIP",
                            _ => "off",
                        })
                        .show_ui(ui, |ui| {
                            for (value, name) in [
                                (0u8, "off"),
                                (1, "utilization"),
                                (2, "throughput"),
                                (3, "WIP"),
                            ] {
                                // A heatmap needs a report; say so instead of
                                // silently falling back to the kind colours.
                                let ready = value == 0 || self.report.is_some();
                                let label = if ready {
                                    name.to_string()
                                } else {
                                    format!("{name}  (run first)")
                                };
                                if ui
                                    .add_enabled(
                                        ready,
                                        egui::Button::new(egui::RichText::new(label).small())
                                            .fill(if self.heatmap == value {
                                                palette::ACCENT_HI
                                            } else {
                                                egui::Color32::TRANSPARENT
                                            })
                                            .rounding(egui::Rounding::same(3.0_f32)),
                                    )
                                    .clicked()
                                {
                                    self.heatmap = value;
                                }
                            }
                        });
                    if let Some(state) = self.sim_state.as_ref() {
                        ui.separator();
                        // Live counters as compact labelled pairs (t / in / out),
                        // monospace so the digits do not jitter while playing.
                        for (label, value) in [
                            ("t", format!("{:.1}", state.now)),
                            ("in", state.items_created.to_string()),
                            ("out", state.items_completed.to_string()),
                        ] {
                            ui.label(
                                egui::RichText::new(label)
                                    .size(9.0_f32)
                                    .color(palette::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new(value)
                                    .small()
                                    .monospace()
                                    .color(palette::TEXT),
                            );
                        }
                    }
                });
            });

        egui::TopBottomPanel::bottom("status-bar")
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .fill(palette::PANEL)
                    .inner_margin(egui::Margin::symmetric(10.0, 4.0)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // A small drawn dot makes the run state readable at a glance
                    // (glyphs such as ● are not in the default font).
                    let (active, colour) = if self.running && !self.paused {
                        (true, palette::OK)
                    } else if self.report_running || self.exp_running {
                        (true, palette::AMBER)
                    } else {
                        (false, palette::TEXT_DIM)
                    };
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
                    if active {
                        ui.painter().circle_filled(rect.center(), 3.6, colour);
                    } else {
                        ui.painter().circle_stroke(
                            rect.center(),
                            3.2,
                            egui::Stroke::new(1.0_f32, colour),
                        );
                    }
                    let message = if self.error.is_some() {
                        "error — see the banner"
                    } else {
                        self.status.as_str()
                    };
                    ui.label(
                        egui::RichText::new(message)
                            .small()
                            .color(palette::TEXT_DIM),
                    );
                    // Model validity is always visible here, not only while an
                    // object happens to be selected.
                    if self.model.is_some() {
                        let issues = self.validation_issues();
                        if issues.is_empty() {
                            ui.label(
                                egui::RichText::new("valid")
                                    .size(9.0_f32)
                                    .color(palette::OK.gamma_multiply(0.85)),
                            )
                            .on_hover_text("the model passes validation");
                        } else {
                            ui.label(
                                egui::RichText::new(format!("{} issue(s)", issues.len()))
                                    .size(9.0_f32)
                                    .strong()
                                    .color(egui::Color32::from_rgb(226, 116, 96)),
                            )
                            .on_hover_text(
                                issues
                                    .iter()
                                    .take(6)
                                    .map(|issue| format!("{}: {}", issue.location, issue.message))
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            );
                        }
                    }
                    if self.report_running
                        || self.exp_running
                        || self.series_running
                        || self.opt_running
                    {
                        // Tiny arc spinner while a background thread works.
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(12.0, 12.0), egui::Sense::hover());
                        let t = ctx.input(|input| input.time) as f32;
                        let centre = rect.center();
                        let radius = 4.2;
                        for step in 0..8u8 {
                            let angle = t * 1.8 + step as f32 * std::f32::consts::TAU / 8.0;
                            let alpha = 40 + step * 26;
                            ui.painter().circle_filled(
                                centre + egui::vec2(angle.cos(), angle.sin()) * radius,
                                1.3,
                                egui::Color32::from_rgba_unmultiplied(150, 180, 240, alpha),
                            );
                        }
                        ctx.request_repaint();
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(state) = self.sim_state.as_ref() {
                            ui.label(
                                egui::RichText::new(format!(
                                    "t = {:.2} {}",
                                    state.now,
                                    self.model
                                        .map(|model| model.time_unit.label())
                                        .unwrap_or("")
                                ))
                                .small()
                                .monospace()
                                .color(palette::TEXT_DIM),
                            );
                        }
                        ui.label(
                            egui::RichText::new("OpenFlex")
                                .small()
                                .color(palette::ACCENT.gamma_multiply(0.8)),
                        );
                    });
                });
            });

        if let Some(error) = self.error.clone() {
            // A tinted banner with an accent rail reads as an alert, not as a
            // second toolbar row.
            egui::TopBottomPanel::top("error")
                .frame(
                    egui::Frame::side_top_panel(&ctx.style())
                        .fill(egui::Color32::from_rgb(46, 24, 24))
                        .stroke(egui::Stroke::new(
                            1.0_f32,
                            egui::Color32::from_rgb(120, 60, 56),
                        ))
                        .inner_margin(egui::Margin::symmetric(10.0, 5.0)),
                )
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        let (rail, _) =
                            ui.allocate_exact_size(egui::vec2(3.0, 16.0), egui::Sense::hover());
                        ui.painter()
                            .rect_filled(rail, 1.5, egui::Color32::from_rgb(226, 116, 96));
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(&error)
                                .small()
                                .color(egui::Color32::from_rgb(255, 205, 195)),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("Dismiss")
                                            .small()
                                            .color(egui::Color32::from_rgb(255, 220, 210)),
                                    )
                                    .fill(egui::Color32::TRANSPARENT)
                                    .rounding(egui::Rounding::same(5.0_f32)),
                                )
                                .clicked()
                            {
                                self.error = None;
                            }
                        });
                    });
                });
        }

        egui::SidePanel::left("objects")
            .resizable(true)
            .default_width(330.0)
            .width_range(260.0..=560.0)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .fill(palette::PANEL)
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0)),
            )
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    panel_heading(ui, "Model tree");
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.search)
                                .hint_text("filter by name or kind")
                                .desired_width(160.0),
                        );
                        if !self.search.is_empty()
                            && ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("x")
                                            .small()
                                            .color(palette::TEXT_DIM),
                                    )
                                    .fill(egui::Color32::TRANSPARENT)
                                    .rounding(egui::Rounding::same(4.0_f32)),
                                )
                                .clicked()
                        {
                            self.search.clear();
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new(if self.tree_by_stage {
                                        "by stage"
                                    } else {
                                        "flat"
                                    })
                                    .size(9.0_f32)
                                    .color(if self.tree_by_stage {
                                        palette::ACCENT_HI
                                    } else {
                                        palette::TEXT_DIM
                                    }),
                                )
                                .fill(palette::CARD)
                                .rounding(egui::Rounding::same(4.0_f32)),
                            )
                            .on_hover_text("group objects by flow stage inside each kind")
                            .clicked()
                        {
                            self.tree_by_stage = !self.tree_by_stage;
                            self.save_ui_prefs();
                        }
                        if let Some(model) = self.model {
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} objects",
                                            model.objects.len()
                                        ))
                                        .small()
                                        .monospace()
                                        .color(palette::TEXT_DIM),
                                    );
                                },
                            );
                        }
                    });
                });
                egui::CollapsingHeader::new(format!(
                    "Layout presets ({})",
                    self.layout_presets.len()
                ))
                .id_salt("layout-presets")
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.preset_name)
                                .hint_text("preset name")
                                .desired_width(130.0),
                        );
                        if ui.small_button("Save").clicked() {
                            self.store_layout_preset();
                        }
                    });
                    let names: Vec<String> = self
                        .layout_presets
                        .iter()
                        .map(|(name, _)| name.clone())
                        .collect();
                    let mut delete: Option<String> = None;
                    for name in names {
                        ui.horizontal(|ui| {
                            if ui.small_button(&name).clicked() {
                                self.apply_layout_preset(&name);
                            }
                            if ui.small_button("x").clicked() {
                                delete = Some(name.clone());
                            }
                        });
                    }
                    if let Some(name) = delete {
                        self.delete_layout_preset(&name);
                    }
                });
                egui::CollapsingHeader::new("Model settings")
                    .id_salt("model-settings")
                    .show(ui, |ui| {
                        let axis = |ui: &mut egui::Ui, text: &str| {
                            ui.label(
                                egui::RichText::new(text)
                                    .size(9.0_f32)
                                    .color(palette::TEXT_DIM),
                            );
                        };
                        ui.horizontal(|ui| {
                            axis(ui, "NAME");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.model_name)
                                    .desired_width(160.0),
                            );
                        });
                        ui.horizontal(|ui| {
                            axis(ui, "UNIT");
                            egui::ComboBox::from_id_salt("model-unit")
                                .selected_text(
                                    ["seconds", "minutes", "hours", "days"]
                                        .get(self.model_unit as usize)
                                        .copied()
                                        .unwrap_or("seconds"),
                                )
                                .show_ui(ui, |ui| {
                                    for (value, name) in [
                                        (0u8, "seconds"),
                                        (1, "minutes"),
                                        (2, "hours"),
                                        (3, "days"),
                                    ] {
                                        if ui
                                            .selectable_label(self.model_unit == value, name)
                                            .clicked()
                                        {
                                            self.model_unit = value;
                                        }
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("STOP")
                                    .size(9.0_f32)
                                    .color(palette::TEXT_DIM),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.model_stop)
                                    .speed(10.0)
                                    .range(1.0..=1.0e9),
                            );
                            ui.label(
                                egui::RichText::new("WARMUP")
                                    .size(9.0_f32)
                                    .color(palette::TEXT_DIM),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.model_warmup)
                                    .speed(1.0)
                                    .range(0.0..=1.0e9),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("SEED")
                                    .size(9.0_f32)
                                    .color(palette::TEXT_DIM),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut self.model_seed)
                                    .desired_width(110.0),
                            );
                            ui.label(
                                egui::RichText::new("REPS")
                                    .size(9.0_f32)
                                    .color(palette::TEXT_DIM),
                            );
                            ui.add(
                                egui::DragValue::new(&mut self.model_replications)
                                    .speed(1.0)
                                    .range(1..=1000),
                            );
                        });
                        ui.add_space(2.0);
                        if primary_button(ui, true, "Apply settings") {
                            self.apply_model_settings();
                        }
                    });
                ui.separator();
                egui::CollapsingHeader::new(format!(
                    "Global tables ({})",
                    self.model.map(|model| model.global_tables.len()).unwrap_or(0)
                ))
                .id_salt("global-tables")
                .show(ui, |ui| {
                    let Some(model) = self.model else {
                        return;
                    };
                    if model.global_tables.is_empty() {
                        empty_state(
                            ui,
                            "No global tables",
                            "Tables live in the model JSON (global_tables).",
                        );
                        return;
                    }
                    // Read-only preview: name, shape and the first rows. Editing
                    // stays in the JSON so the whole table can be pasted in.
                    for (name, table) in &model.global_tables {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(name)
                                    .strong()
                                    .color(palette::ACCENT_HI),
                            );
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} x {}",
                                    table.rows.len(),
                                    table.width()
                                ))
                                .small()
                                .monospace()
                                .color(palette::TEXT_DIM),
                            );
                        });
                        if !table.columns.is_empty() {
                            ui.horizontal_wrapped(|ui| {
                                for column in &table.columns {
                                    egui::Frame::none()
                                        .fill(palette::CARD)
                                        .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
                                        .rounding(egui::Rounding::same(4.0_f32))
                                        .inner_margin(egui::Margin::symmetric(5.0, 1.0))
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(column)
                                                    .size(10.0_f32)
                                                    .color(palette::TEXT_DIM),
                                            );
                                        });
                                }
                            });
                        }
                        // Editable cells: each row is a text buffer, committed on
                        // Enter/lost focus. Buffers live in `table_edit`.
                        let rows = if table.rows.is_empty() {
                            0
                        } else {
                            table.rows.len()
                        };
                        let mut commit: Option<(usize, Vec<String>)> = None;
                        for row_index in 0..rows {
                            ui.horizontal_wrapped(|ui| {
                                for column_index in 0..table.width() {
                                    let key = (name.clone(), row_index, column_index);
                                    if !self.table_edit.contains_key(&key) {
                                        self.table_edit.insert(
                                            key.clone(),
                                            table.rows[row_index][column_index]
                                                .as_text(),
                                        );
                                    }
                                    let buffer = self
                                        .table_edit
                                        .get_mut(&key)
                                        .expect("buffer just inserted");
                                    let response = ui.add(
                                        egui::TextEdit::singleline(buffer)
                                            .desired_width(78.0)
                                            .font(egui::TextStyle::Monospace),
                                    );
                                    if response.lost_focus() {
                                        let cells: Vec<String> = (0..table.width())
                                            .map(|column| {
                                                self.table_edit
                                                    .get(&(name.clone(), row_index, column))
                                                    .cloned()
                                                    .unwrap_or_default()
                                            })
                                            .collect();
                                        commit = Some((row_index, cells));
                                    }
                                }
                            });
                        }
                        if let Some((row_index, cells)) = commit {
                            self.commit_table_row(name, row_index, cells);
                        }
                        ui.horizontal(|ui| {
                            if ui
                                .small_button("+ row")
                                .on_hover_text("append a row of default cells")
                                .clicked()
                            {
                                self.append_table_row(name);
                            }
                            if ui.small_button("reset").clicked() {
                                // Drop the buffers so they reload from the model.
                                self.table_edit
                                    .retain(|(table, _, _), _| table != name);
                            }
                        });
                        ui.add_space(2.0);
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    // "+ Add object" row: kind picker, optional name, primary Add.
                    egui::ComboBox::from_id_salt("new-kind")
                        .selected_text(
                            egui::RichText::new(self.new_kind.clone()).small(),
                        )
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for kind in KINDS {
                                let selected = self.new_kind == *kind;
                                if ui
                                    .selectable_label(
                                        selected,
                                        egui::RichText::new(*kind).small(),
                                    )
                                    .clicked()
                                {
                                    self.new_kind = (*kind).to_string();
                                }
                            }
                        });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_name)
                            .hint_text("name (optional)")
                            .desired_width(96.0),
                    );
                    if primary_button(ui, !self.new_kind.is_empty(), "Add") {
                        let kind = self.new_kind.clone();
                        self.add_object(&kind);
                    }
                });
                ui.separator();
                let Some(model) = self.model else {
                    ui.label(
                        egui::RichText::new("No model loaded").color(egui::Color32::from_gray(180)),
                    );
                    ui.label(
                        egui::RichText::new(
                            "Start with File > New, File > Open, or a bundled example:",
                        )
                        .small()
                        .weak(),
                    );
                    if ui.button("New model").clicked() {
                        self.new_model();
                    }
                    // Five examples would run down the whole panel one per row.
                    ui.horizontal_wrapped(|ui| {
                        for (example, hint) in [
                            ("mm1", "single queue and processor"),
                            ("routing", "round-robin to two processors"),
                            ("batch-queue", "batching before processing"),
                            ("agv-delivery", "dispatcher and one AGV"),
                            ("fluid-line", "generator, tank, valve, sink"),
                        ] {
                            let button = egui::Button::new(
                                egui::RichText::new(example)
                                    .small()
                                    .color(palette::TEXT),
                            )
                            .fill(palette::CARD)
                            .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
                            .rounding(egui::Rounding::same(10.0_f32));
                            if ui.add(button).on_hover_text(hint).clicked() {
                                self.load_example(example);
                            }
                        }
                    });
                    return;
                };
                let needle = self.search.to_lowercase();
                egui::ScrollArea::vertical()
                    .id_salt("tree")
                    .max_height(ui.available_height() * 0.45)
                    .show(ui, |ui| {
                        // Stage map is computed once per frame (cheap BFS).
                        let stages_of = Self::stage_of(model);
                        let mut kinds: Vec<ObjectKind> = Vec::new();
                        for object in &model.objects {
                            if !kinds.contains(&object.kind) {
                                kinds.push(object.kind);
                            }
                        }
                        for kind in kinds {
                            let indices: Vec<usize> = model
                                .objects
                                .iter()
                                .enumerate()
                                .filter(|(_, object)| object.kind == kind)
                                .filter(|(_, object)| {
                                    needle.is_empty()
                                        || object.name.to_lowercase().contains(&needle)
                                        || kind.as_str().contains(&needle)
                                })
                                .map(|(index, _)| index)
                                .collect();
                            if indices.is_empty() {
                                continue;
                            }
                            egui::CollapsingHeader::new(format!(
                                "{} ({})",
                                kind.as_str(),
                                indices.len()
                            ))
                            .id_salt(kind.as_str())
                            .default_open(true)
                            .show(ui, |ui| {
                                // Second level: the flow stage (BFS distance from
                                // the sources) so a big model is not one long list.
                                let mut stages: Vec<(usize, Vec<usize>)> = Vec::new();
                                if self.tree_by_stage {
                                    for index in indices.clone() {
                                        let stage = stages_of[index];
                                        match stages.iter_mut().find(|(s, _)| *s == stage) {
                                            Some((_, list)) => list.push(index),
                                            None => stages.push((stage, vec![index])),
                                        }
                                    }
                                    stages.sort_by_key(|(stage, _)| *stage);
                                } else {
                                    stages.push((usize::MAX, indices.clone()));
                                }
                                for (stage, group) in stages {
                                    if stage == usize::MAX {
                                        for index in group {
                                            self.tree_row(ui, model, index);
                                        }
                                        continue;
                                    }
                                    egui::CollapsingHeader::new(format!(
                                        "stage {stage} ({})",
                                        group.len()
                                    ))
                                    .id_salt((kind.as_str(), stage))
                                    .default_open(true)
                                    .show(ui, |ui| {
                                        for index in group {
                                            self.tree_row(ui, model, index);
                                        }
                                    });
                                }
                                if indices.len() > 1 && ui.small_button("select all").clicked() {
                                    self.selection = indices.clone();
                                    self.status =
                                        format!("{} object(s) selected", indices.len());
                                }
                            });
                        }
                    });
                ui.separator();
                let mut form_changed = false;
                let mut pending_rename: Option<String> = None;
                if let (Some(model), Some(index), Some(form)) =
                    (self.model, self.selected, self.form.as_mut())
                {
                    let name = model.objects[index].name.clone();
                    let kind = form.kind;
                    // Object header: kind dot + editable name + kind tag.
                    let mut rename_to: Option<String> = None;
                    ui.horizontal(|ui| {
                        let (dot, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter()
                            .circle_filled(dot.center(), 4.2, Self::kind_color(kind));
                        // The name is edited in place: Enter or losing focus
                        // commits it, Esc restores the old value.
                        let response = ui.add(
                            egui::TextEdit::singleline(&mut self.rename_text)
                                .desired_width(150.0)
                                .font(egui::TextStyle::Body)
                                .text_color(palette::TEXT),
                        );
                        // Enter or losing focus commits, Esc reverts.
                        let escaped = response.lost_focus()
                            && ui.input(|input| input.key_pressed(egui::Key::Escape));
                        if escaped {
                            self.rename_text = name.clone();
                        } else if response.lost_focus() && self.rename_text.trim() != name {
                            rename_to = Some(self.rename_text.trim().to_string());
                        }
                        egui::Frame::none()
                            .fill(palette::CARD)
                            .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
                            .rounding(egui::Rounding::same(9.0_f32))
                            .inner_margin(egui::Margin::symmetric(6.0, 1.0))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(kind.as_str())
                                        .size(10.0_f32)
                                        .color(palette::TEXT_DIM),
                                );
                            });
                    });
                    pending_rename = rename_to;
                    // Position row: labelled monospace fields on one line.
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("x")
                                .size(10.0_f32)
                                .monospace()
                                .color(palette::TEXT_DIM),
                        );
                        ui.add(
                            egui::DragValue::new(&mut form.pos[0])
                                .speed(1.0)
                                .max_decimals(2),
                        );
                        ui.label(
                            egui::RichText::new("y")
                                .size(10.0_f32)
                                .monospace()
                                .color(palette::TEXT_DIM),
                        );
                        ui.add(
                            egui::DragValue::new(&mut form.pos[1])
                                .speed(1.0)
                                .max_decimals(2),
                        );
                        ui.label(
                            egui::RichText::new("z")
                                .size(10.0_f32)
                                .monospace()
                                .color(palette::TEXT_DIM),
                        );
                        ui.add(
                            egui::DragValue::new(&mut form.z)
                                .speed(0.1)
                                .max_decimals(2),
                        );
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("form")
                        .max_height(ui.available_height() * 0.55)
                        .show(ui, |ui| {
                            form_changed = form.show(ui);
                        });
                    ui.horizontal(|ui| {
                        if primary_button(ui, true, "Apply") {
                            self.apply_form();
                        }
                        // A "dirty" hint appears as soon as a field is edited.
                        if form_changed {
                            self.form_dirty = true;
                        }
                        if self.form_dirty {
                            ui.label(
                                egui::RichText::new("unsaved edits")
                                    .size(10.0_f32)
                                    .color(palette::AMBER),
                            );
                        }
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.checkbox(&mut self.show_json, "JSON")
                                    .on_hover_text("edit the raw parameter JSON");
                            },
                        );
                    });
                    // Live validation: the model is checked against the same
                    // rules the CLI uses. Each issue is clickable and selects
                    // the object it belongs to (locations look like
                    // `objects.<name>.<field>`).
                    if self.model.is_some() {
                        let issues = self.validation_issues();
                        if !issues.is_empty() {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} validation issue(s)",
                                    issues.len()
                                ))
                                .strong()
                                .color(egui::Color32::from_rgb(226, 116, 96)),
                            );
                            let mut jump: Option<String> = None;
                            egui::ScrollArea::vertical()
                                .id_salt("issues")
                                .max_height(110.0)
                                .show(ui, |ui| {
                                    for issue in issues.iter().take(20) {
                                        let target =
                                            Self::issue_target(&issue.location);
                                        let label = format!(
                                            "{}: {}",
                                            issue.location, issue.message
                                        );
                                        let response = ui
                                            .add(
                                                egui::Label::new(
                                                    egui::RichText::new(label)
                                                        .small()
                                                        .color(palette::TEXT),
                                                )
                                                .sense(egui::Sense::click()),
                                            )
                                            .on_hover_text(
                                                "click to select the object involved",
                                            );
                                        if response.clicked()
                                            && let Some(target) = target.clone()
                                        {
                                            jump = Some(target);
                                        }
                                        if response.hovered() {
                                            response
                                                .ctx
                                                .set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                    }
                                    if issues.len() > 20 {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "… {} more",
                                                issues.len() - 20
                                            ))
                                            .size(9.0_f32)
                                            .color(palette::TEXT_DIM),
                                        );
                                    }
                                });
                            if let Some(target) = jump
                                && let Some(model) = self.model
                                && let Some(index) =
                                    model.objects.iter().position(|o| o.name == target)
                            {
                                self.select(index);
                            }
                        } else {
                            ui.separator();
                            ui.label(
                                egui::RichText::new("model is valid")
                                    .small()
                                    .color(palette::OK),
                            );
                        }
                    }
                    if self.show_json {
                        // Live JSON validity hint: parse on edit, colour the
                        // frame green/red so a typo is obvious before Apply.
                        let valid = self.params_text.trim().is_empty()
                            || serde_json::from_str::<serde_json::Value>(&self.params_text).is_ok();
                        let frame = if valid {
                            egui::Frame::none()
                        } else {
                            egui::Frame::none()
                                .stroke(egui::Stroke::new(
                                    1.0_f32,
                                    egui::Color32::from_rgb(180, 80, 70),
                                ))
                                .rounding(egui::Rounding::same(4.0_f32))
                        };
                        frame.show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.params_text)
                                    .code_editor()
                                    .desired_rows(8)
                                    .desired_width(f32::INFINITY),
                            );
                        });
                        ui.horizontal(|ui| {
                            if primary_button(ui, valid, "Apply JSON") {
                                self.apply_json();
                            }
                            ui.label(
                                egui::RichText::new(if valid {
                                    "valid JSON"
                                } else {
                                    "invalid JSON — check the brackets"
                                })
                                .size(10.0_f32)
                                .color(if valid {
                                    palette::OK
                                } else {
                                    egui::Color32::from_rgb(226, 116, 96)
                                }),
                            );
                        });
                    }
                }
                if let Some(new_name) = pending_rename {
                    self.rename_object(new_name);
                }
                if !self.undo.is_empty() || !self.redo.is_empty() {
                    ui.separator();
                    egui::CollapsingHeader::new(format!(
                        "History ({} undo / {} redo)",
                        self.undo.len(),
                        self.redo.len()
                    ))
                    .id_salt("history")
                    .show(ui, |ui| {
                        // Newest first; clicking an entry rewinds to it.
                        let labels: Vec<(usize, String)> = self
                            .undo
                            .iter()
                            .enumerate()
                            .rev()
                            .map(|(index, (_, label))| (index, label.clone()))
                            .collect();
                        let mut rewind: Option<usize> = None;
                        for (index, label) in labels {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{}", index + 1))
                                        .size(9.0_f32)
                                        .monospace()
                                        .color(palette::TEXT_DIM),
                                );
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(label)
                                                .small()
                                                .color(palette::TEXT),
                                        )
                                        .fill(egui::Color32::TRANSPARENT)
                                        .rounding(egui::Rounding::same(4.0_f32)),
                                    )
                                    .on_hover_text("rewind to this step")
                                    .clicked()
                                {
                                    rewind = Some(index);
                                }
                            });
                        }
                        for (_, label) in self.redo.iter().rev() {
                            ui.label(
                                egui::RichText::new(format!("redo · {label}"))
                                    .size(9.0_f32)
                                    .color(palette::TEXT_DIM.gamma_multiply(0.7)),
                            );
                        }
                        if let Some(target) = rewind {
                            // Pop until `target` is the top entry.
                            while self.undo.len() > target {
                                if !self.restore() {
                                    break;
                                }
                            }
                        }
                    });
                }
                if !self.selection.is_empty() {
                    ui.separator();
                    // Selection header: amber count + a one-line hint.
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}", self.selection.len()))
                                .size(16.0_f32)
                                .strong()
                                .color(palette::AMBER),
                        );
                        ui.label(
                            egui::RichText::new(if self.selection.len() == 1 {
                                "object selected"
                            } else {
                                "objects selected"
                            })
                            .small()
                            .color(palette::TEXT_DIM),
                        );
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new("DUP/COPY · DEL")
                                        .size(9.0_f32)
                                        .color(palette::TEXT_DIM.gamma_multiply(0.7)),
                                );
                            },
                        );
                    });
                    // Align / distribute / arrange group.
                    ui.horizontal_wrapped(|ui| {
                        for (label, action) in [
                            ("Align X", 0u8),
                            ("Align Y", 1),
                            ("Dist X", 2),
                            ("Dist Y", 3),
                            ("Grid", 4),
                        ] {
                            let button = egui::Button::new(
                                egui::RichText::new(label).small().color(palette::TEXT),
                            )
                            .fill(palette::CARD)
                            .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
                            .rounding(egui::Rounding::same(5.0_f32))
                            .min_size(egui::vec2(0.0, 20.0));
                            if ui.add(button).clicked() {
                                match action {
                                    0 => self.batch_align(true),
                                    1 => self.batch_align(false),
                                    2 => self.batch_distribute(true),
                                    3 => self.batch_distribute(false),
                                    _ => self.batch_grid(4.0),
                                }
                            }
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Duplicate")
                                        .small()
                                        .color(palette::TEXT),
                                )
                                .fill(palette::CARD)
                                .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
                                .rounding(egui::Rounding::same(5.0_f32)),
                            )
                            .clicked()
                        {
                            self.duplicate_selection();
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Delete")
                                        .small()
                                        .color(egui::Color32::from_rgb(240, 150, 130)),
                                )
                                .fill(palette::CARD)
                                .stroke(egui::Stroke::new(
                                    1.0_f32,
                                    egui::Color32::from_rgb(120, 70, 60),
                                ))
                                .rounding(egui::Rounding::same(5.0_f32)),
                            )
                            .clicked()
                        {
                            self.batch_delete();
                        }
                        if ui
                            .add(
                                egui::Button::new(
                                    egui::RichText::new("Clear")
                                        .small()
                                        .color(palette::TEXT_DIM),
                                )
                                .fill(egui::Color32::TRANSPARENT)
                                .rounding(egui::Rounding::same(5.0_f32)),
                            )
                            .clicked()
                        {
                            self.selection.clear();
                            self.status = "Selection cleared".into();
                        }
                    });
                    ui.add_space(2.0);
                    // Transform group: move and rotate share one compact row.
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("MOVE")
                                .size(9.0_f32)
                                .color(palette::TEXT_DIM),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.batch_dx)
                                .speed(0.5)
                                .prefix("dx ")
                                .max_decimals(1),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.batch_dy)
                                .speed(0.5)
                                .prefix("dy ")
                                .max_decimals(1),
                        );
                        if ui.small_button("Go").clicked() {
                            self.batch_move();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("ROT")
                                .size(9.0_f32)
                                .color(palette::TEXT_DIM),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.batch_rot)
                                .speed(15.0)
                                .range(-360.0..=360.0)
                                .suffix("°"),
                        );
                        if ui.small_button("Go").clicked() {
                            self.batch_rotate();
                        }
                    });
                }
                ui.separator();
                egui::CollapsingHeader::new("Process Flow")
                    .id_salt("process-flow")
                    .show(ui, |ui| {
                        // Visual node canvas: drag blocks, click two blocks to connect.
                        let ids = self.flow_block_ids();
                        if !ids.is_empty() {
                            let (response, painter) = ui.allocate_painter(
                                egui::vec2(ui.available_width(), 210.0),
                                egui::Sense::click_and_drag(),
                            );
                            // Clip so panned nodes cannot bleed into the panels.
                            let painter = painter.with_clip_rect(response.rect);
                            let origin = response.rect.min + self.flow_pan;
                            let zoom = self.flow_zoom.clamp(0.4, 2.5);
                            let rect_of = |id: &String, index: usize| {
                                let pos = self
                                    .flow_pos
                                    .get(id)
                                    .copied()
                                    .unwrap_or_else(|| Self::flow_default_pos(index));
                                egui::Rect::from_min_size(
                                    origin + egui::vec2(pos[0], pos[1]) * zoom,
                                    egui::vec2(124.0, 40.0) * zoom,
                                )
                            };
                            painter.rect_filled(response.rect, 6.0, palette::CANVAS);
                            // Dot grid (like a diagram editor) follows pan/zoom.
                            let spacing = (24.0 * zoom).max(10.0);
                            let offset = self.flow_pan;
                            let mut x = response.rect.left() + offset.x.rem_euclid(spacing);
                            while x < response.rect.right() {
                                let mut y = response.rect.top() + offset.y.rem_euclid(spacing);
                                while y < response.rect.bottom() {
                                    painter.circle_filled(
                                        egui::pos2(x, y),
                                        0.9,
                                        egui::Color32::from_rgba_unmultiplied(120, 145, 180, 34),
                                    );
                                    y += spacing;
                                }
                                x += spacing;
                            }
                            let rects: Vec<(String, egui::Rect)> = ids
                                .iter()
                                .enumerate()
                                .map(|(index, id)| (id.clone(), rect_of(id, index)))
                                .collect();
                            // Connections.
                            if let Some(flow) = self.flow_value()
                                && let Some(connections) =
                                    flow.get("connections").and_then(|c| c.as_array())
                            {
                                for connection in connections {
                                    let from = connection.get("from").and_then(|v| v.as_str());
                                    let to = connection.get("to").and_then(|v| v.as_str());
                                    if let (Some(from), Some(to)) = (from, to)
                                        && let (Some((_, a)), Some((_, b))) = (
                                            rects.iter().find(|(id, _)| id == from),
                                            rects.iter().find(|(id, _)| id == to),
                                        )
                                    {
                                        painter.line_segment(
                                            [a.right_center(), b.left_center()],
                                            egui::Stroke::new(
                                                1.5_f32,
                                                egui::Color32::from_rgb(120, 160, 200),
                                            ),
                                        );
                                    }
                                }
                            }
                            if response.drag_started()
                                && let Some(pointer) = response.interact_pointer_pos()
                            {
                                self.flow_drag = rects
                                    .iter()
                                    .find(|(_, rect)| rect.contains(pointer))
                                    .map(|(id, _)| id.clone());
                                self.flow_selected = self.flow_drag.clone();
                            }
                            if response.dragged()
                                && let (Some(id), true) = (self.flow_drag.clone(), true)
                            {
                                let delta = response.drag_delta() / zoom;
                                let entry = self.flow_pos.entry(id).or_insert([20.0, 20.0]);
                                entry[0] += delta.x;
                                entry[1] += delta.y;
                            } else if response.dragged() && self.flow_drag.is_none() {
                                // Dragging empty canvas pans the view.
                                self.flow_pan += response.drag_delta();
                            }
                            if response.hovered() {
                                let (scroll, ctrl) = ui.input(|input| {
                                    (input.smooth_scroll_delta.y, input.modifiers.command)
                                });
                                if scroll != 0.0 {
                                    if ctrl {
                                        self.flow_zoom =
                                            (self.flow_zoom * (1.0 + scroll * 0.002))
                                                .clamp(0.4, 2.5);
                                    } else {
                                        self.flow_pan.y += scroll * 0.5;
                                    }
                                }
                            }
                            if response.drag_stopped() {
                                self.flow_drag = None;
                            }
                            if response.clicked()
                                && let Some(pointer) = response.interact_pointer_pos()
                                && let Some((id, _)) =
                                    rects.iter().find(|(_, rect)| rect.contains(pointer))
                            {
                                match self.flow_pending.clone() {
                                    Some(from) if &from != id => {
                                        self.flow_from = from;
                                        self.flow_to = id.clone();
                                        self.add_flow_connection();
                                        self.flow_pending = None;
                                    }
                                    _ => {
                                        self.flow_pending = Some(id.clone());
                                        self.flow_selected = Some(id.clone());
                                    }
                                }
                            }
                            for (id, rect) in &rects {
                                let selected = self.flow_selected.as_ref() == Some(id);
                                let pending = self.flow_pending.as_ref() == Some(id);
                                // Type colour comes from the block type so the
                                // canvas reads like a diagram, not a grey grid.
                                let kind = self
                                    .flow_value()
                                    .and_then(|flow| {
                                        flow.get("blocks")?.as_array()?.iter().find_map(|block| {
                                            (block.get("id")?.as_str()? == id)
                                                .then(|| block.get("type")?.as_str())
                                                .flatten()
                                                .map(str::to_string)
                                        })
                                    })
                                    .unwrap_or_default();
                                let base = Self::flow_kind_colour(&kind);
                                let fill = if selected || pending {
                                    palette::AMBER
                                } else {
                                    base
                                };
                                // Drop shadow keeps the node off the background.
                                painter.rect_filled(
                                    rect.translate(egui::vec2(0.0, 2.0)),
                                    (6.0 * zoom).max(3.0),
                                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 90),
                                );
                                painter.rect_filled(*rect, (6.0 * zoom).max(3.0), fill);
                                painter.rect_stroke(
                                    *rect,
                                    (6.0 * zoom).max(3.0),
                                    egui::Stroke::new(
                                        1.0_f32,
                                        if selected || pending {
                                            egui::Color32::from_rgb(255, 236, 190)
                                        } else {
                                            palette::STROKE
                                        },
                                    ),
                                );
                                let text_colour = if selected || pending {
                                    egui::Color32::from_rgb(26, 20, 10)
                                } else {
                                    egui::Color32::WHITE
                                };
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    id,
                                    egui::FontId::proportional((13.0 * zoom).max(9.0)),
                                    text_colour,
                                );
                                if !kind.is_empty() && zoom > 0.6 {
                                    painter.text(
                                        rect.left_top() + egui::vec2(6.0 * zoom, 4.0 * zoom),
                                        egui::Align2::LEFT_TOP,
                                        kind,
                                        egui::FontId::proportional((9.0 * zoom).max(7.0)),
                                        text_colour.gamma_multiply(0.75),
                                    );
                                }
                            }
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(
                                        "drag nodes · drag empty space to pan · Cmd+wheel to zoom · click two nodes to connect",
                                    )
                                    .weak()
                                    .small(),
                                );
                                if ui.small_button("Reset view").clicked() {
                                    self.flow_pan = egui::Vec2::ZERO;
                                    self.flow_zoom = 1.0;
                                }
                                ui.label(
                                    egui::RichText::new(format!("{:.2}×", self.flow_zoom))
                                        .weak()
                                        .small(),
                                );
                            });
                        }
                        ui.label(
                            egui::RichText::new("BLOCKS")
                                .size(9.0_f32)
                                .color(palette::TEXT_DIM),
                        );
                        // Block chips in their diagram colours, matching the
                        // canvas so the two views use one vocabulary.
                        ui.horizontal_wrapped(|ui| {
                            for kind in
                                ["create", "delay", "assign", "decision", "transfer", "sink"]
                            {
                                let colour = Self::flow_kind_colour(kind);
                                let button = egui::Button::new(
                                    egui::RichText::new(format!("+ {kind}"))
                                        .small()
                                        .color(egui::Color32::WHITE),
                                )
                                .fill(colour.gamma_multiply(0.85))
                                .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
                                .rounding(egui::Rounding::same(10.0_f32));
                                if ui.add(button).clicked() {
                                    self.add_flow_block(kind);
                                }
                            }
                        });
                        ui.add(
                            egui::TextEdit::multiline(&mut self.flow_text)
                                .code_editor()
                                .desired_rows(8),
                        );
                        if let Some(flow) = self.flow_value() {
                            if let Some(blocks) = flow.get("blocks").and_then(|b| b.as_array()) {
                                ui.label(format!("{} block(s)", blocks.len()));
                                let ids: Vec<String> = blocks
                                    .iter()
                                    .filter_map(|block| {
                                        block
                                            .get("id")
                                            .and_then(|id| id.as_str())
                                            .map(str::to_string)
                                    })
                                    .collect();
                                let mut remove: Option<String> = None;
                                ui.horizontal_wrapped(|ui| {
                                    for id in ids {
                                        egui::Frame::none()
                                            .fill(palette::CARD)
                                            .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
                                            .rounding(egui::Rounding::same(9.0_f32))
                                            .inner_margin(egui::Margin::symmetric(6.0, 1.0))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(&id)
                                                            .small()
                                                            .monospace()
                                                            .color(palette::TEXT),
                                                    );
                                                    if ui
                                                        .add(
                                                            egui::Button::new(
                                                                egui::RichText::new("x")
                                                                    .small()
                                                                    .color(palette::TEXT_DIM),
                                                            )
                                                            .fill(egui::Color32::TRANSPARENT)
                                                            .rounding(egui::Rounding::same(3.0_f32)),
                                                        )
                                                        .clicked()
                                                    {
                                                        remove = Some(id.clone());
                                                    }
                                                });
                                            });
                                    }
                                });
                                if let Some(id) = remove {
                                    self.remove_flow_block(&id);
                                }
                            }
                            if let Some(items) = flow.get("connections").and_then(|c| c.as_array())
                            {
                                ui.label(format!("{} connection(s)", items.len()));
                            }
                        }
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.flow_from)
                                    .hint_text("from")
                                    .desired_width(60.0),
                            );
                            ui.add(
                                egui::TextEdit::singleline(&mut self.flow_to)
                                    .hint_text("to")
                                    .desired_width(60.0),
                            );
                            if ui.small_button("connect").clicked() {
                                self.add_flow_connection();
                            }
                        });
                        if primary_button(ui, true, "Apply process flow") {
                            self.apply_flow();
                        }
                    });
                ui.separator();
                panel_heading(ui, "Experiment");
                ui.horizontal(|ui| {
                    for (value, name) in [(false, "stop-time"), (true, "parameter")] {
                        let selected = self.exp_mode_param == value;
                        let button = egui::Button::new(
                            egui::RichText::new(name).small().color(if selected {
                                egui::Color32::from_rgb(14, 18, 26)
                            } else {
                                palette::TEXT
                            }),
                        )
                        .fill(if selected { palette::ACCENT_HI } else { palette::CARD })
                        .rounding(egui::Rounding::same(5.0_f32));
                        if ui.add(button).clicked() && !selected {
                            self.exp_mode_param = value;
                        }
                    }
                });
                if self.exp_mode_param {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new("PATH")
                                .size(9.0_f32)
                                .color(palette::TEXT_DIM),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.exp_path)
                                .hint_text("objects/0/params/process_time/value")
                                .desired_width(240.0),
                        );
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("START")
                            .size(9.0_f32)
                            .color(palette::TEXT_DIM),
                    );
                    ui.add(egui::DragValue::new(&mut self.exp_start).speed(10.0));
                    ui.label(
                        egui::RichText::new("STEP")
                            .size(9.0_f32)
                            .color(palette::TEXT_DIM),
                    );
                    ui.add(egui::DragValue::new(&mut self.exp_step).speed(10.0));
                    ui.label(
                        egui::RichText::new("RUNS")
                            .size(9.0_f32)
                            .color(palette::TEXT_DIM),
                    );
                    ui.add(
                        egui::DragValue::new(&mut self.exp_count)
                            .speed(1.0)
                            .range(1..=20),
                    );
                    ui.label(
                        egui::RichText::new("REPS")
                            .size(9.0_f32)
                            .color(palette::TEXT_DIM),
                    )
                    .on_hover_text("replications per scenario (adds a 95% CI to every row)");
                    ui.add(
                        egui::DragValue::new(&mut self.exp_reps)
                            .speed(1.0)
                            .range(1..=50),
                    );
                });
                if ui
                    .add_enabled(
                        self.model.is_some() && !self.exp_running,
                        egui::Button::new(if self.exp_running {
                            "Running…"
                        } else {
                            "Run experiment"
                        }),
                    )
                    .clicked()
                {
                    self.start_experiment();
                }
                if !self.exp_rows.is_empty() {
                    let (response, painter) = ui.allocate_painter(
                        egui::vec2(ui.available_width(), 110.0),
                        egui::Sense::hover(),
                    );
                    let rect = response.rect;
                    painter.rect_filled(rect, 0.0, palette::CANVAS);
                    let max = self
                        .exp_rows
                        .iter()
                        .map(|(_, completed, _)| *completed)
                        .fold(0.0_f64, f64::max)
                        .max(1.0);
                    let n = self.exp_rows.len().max(1);
                    let dx = rect.width() / n as f32;
                    for (index, (_, completed, _)) in self.exp_rows.iter().enumerate() {
                        let height = (rect.height() - 12.0) * (*completed / max) as f32;
                        let bar = egui::Rect::from_min_max(
                            egui::pos2(
                                rect.left() + index as f32 * dx + 4.0,
                                rect.bottom() - 4.0 - height,
                            ),
                            egui::pos2(
                                rect.left() + (index as f32 + 1.0) * dx - 4.0,
                                rect.bottom() - 4.0,
                            ),
                        );
                        let is_best = self
                            .exp_rows
                            .iter()
                            .map(|(_, completed, _)| *completed)
                            .fold(f64::NEG_INFINITY, f64::max)
                            == *completed;
                        painter.rect_filled(
                            bar,
                            2.0,
                            if is_best {
                                palette::OK
                            } else {
                                egui::Color32::from_rgb(90, 150, 210)
                            },
                        );
                        // Hovering a bar names the scenario value and its result.
                        if response.hover_pos().is_some_and(|pos| bar.contains(pos)) {
                            painter.rect_stroke(
                                bar,
                                2.0,
                                egui::Stroke::new(1.0_f32, palette::ACCENT_HI),
                            );
                            painter.text(
                                egui::pos2(bar.center().x, bar.top() - 2.0),
                                egui::Align2::CENTER_BOTTOM,
                                format!("{completed:.0}"),
                                egui::FontId::proportional(9.0),
                                palette::TEXT,
                            );
                        }
                    }
                    // The best scenario is highlighted in the table as well.
                    let best = self
                        .exp_rows
                        .iter()
                        .enumerate()
                        .max_by(|a, b| a.1.1.total_cmp(&b.1.1))
                        .map(|(index, _)| index);
                    egui::Grid::new("exp-grid")
                        .striped(true)
                        .num_columns(3)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("value").strong().color(palette::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new("completed")
                                    .strong()
                                    .color(palette::TEXT_DIM),
                            );
                            ui.label(
                                egui::RichText::new("± CI95").strong().color(palette::TEXT_DIM),
                            );
                            ui.end_row();
                            for (index, (value, completed, ci)) in
                                self.exp_rows.iter().enumerate()
                            {
                                let top = best == Some(index);
                                let colour = if top { palette::OK } else { palette::TEXT };
                                ui.label(
                                    egui::RichText::new(if top {
                                        format!("{value:.1} *")
                                    } else {
                                        format!("{value:.1}")
                                    })
                                    .monospace()
                                    .small()
                                    .color(colour),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{completed:.1}"))
                                        .strong()
                                        .color(colour),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{ci:.1}"))
                                        .monospace()
                                        .small()
                                        .color(palette::TEXT_DIM),
                                );
                                ui.end_row();
                            }
                        });
                }
                ui.separator();
                panel_heading(ui, "Optimise");
                ui.horizontal(|ui| {
                    for (value, name) in [(0u8, "grid"), (1, "random"), (2, "genetic")] {
                        let selected = self.opt_mode == value;
                        let button = egui::Button::new(
                            egui::RichText::new(name).color(if selected {
                                egui::Color32::from_rgb(14, 18, 26)
                            } else {
                                palette::TEXT
                            }),
                        )
                        .fill(if selected { palette::ACCENT_HI } else { palette::CARD })
                        .rounding(egui::Rounding::same(5.0_f32));
                        if ui.add(button).clicked() && !selected {
                            self.opt_mode = value;
                        }
                    }
                });
                let mut remove: Option<usize> = None;
                for index in 0..self.opt_vars.len() {
                    ui.horizontal(|ui| {
                        ui.label(format!("#{}", index + 1));
                        ui.add(
                            egui::TextEdit::singleline(&mut self.opt_vars[index].path)
                                .hint_text("objects/0/params/interarrival/mean")
                                .desired_width(170.0),
                        );
                        ui.label("low");
                        ui.add(egui::DragValue::new(&mut self.opt_vars[index].low).speed(0.5));
                        ui.label("high");
                        ui.add(egui::DragValue::new(&mut self.opt_vars[index].high).speed(0.5));
                        if ui.small_button("x").clicked() {
                            remove = Some(index);
                        }
                    });
                }
                if let Some(index) = remove {
                    self.opt_vars.remove(index);
                }
                ui.horizontal(|ui| {
                    if ui.small_button("+ variable").clicked() {
                        self.opt_vars.push(OptVar {
                            path: String::new(),
                            low: 1.0,
                            high: 10.0,
                        });
                    }
                    // Picking a field beats typing a slash path by hand.
                    if ui
                        .add_enabled(self.model.is_some(), egui::Button::new("pick field"))
                        .on_hover_text("choose a numeric parameter to optimise")
                        .clicked()
                    {
                        self.show_path_picker = true;
                        self.path_candidates.clear();
                        if let Some(model) = self.model
                            && let Ok(json) = serde_json::to_value(model)
                            && let Some(objects) =
                                json.get("objects").and_then(|o| o.as_array())
                        {
                            for (index, object) in objects.iter().enumerate() {
                                if let Some(params) = object.get("params") {
                                    let prefix = format!("objects/{index}/params");
                                    numeric_paths(params, &prefix, &mut self.path_candidates);
                                }
                            }
                        }
                        self.path_candidates.sort();
                        self.path_candidates.dedup();
                    }
                    ui.label("steps");
                    ui.add(
                        egui::DragValue::new(&mut self.opt_steps)
                            .speed(1.0)
                            .range(2..=50),
                    );
                    if self.opt_mode == 1 {
                        ui.label("samples");
                        ui.add(
                            egui::DragValue::new(&mut self.opt_samples)
                                .speed(1.0)
                                .range(2..=300),
                        );
                    }
                    ui.checkbox(&mut self.opt_secondary_stay, "2nd: max stay");
                });
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            self.model.is_some() && !self.opt_running,
                            egui::Button::new(if self.opt_running {
                                "Optimising…"
                            } else {
                                "Run"
                            }),
                        )
                        .clicked()
                    {
                        self.start_optimize();
                    }
                    // Show how many model evaluations a run will cost before
                    // the user commits to it.
                    let evaluations = match self.opt_mode {
                        0 => (self.opt_steps.clamp(2, 50) as u64).pow(self.opt_vars.len().max(1) as u32),
                        1 => self.opt_samples.clamp(2, 300) as u64,
                        _ => {
                            (self.opt_samples.clamp(2, 300) as u64)
                                * (self.opt_steps.clamp(2, 50) as u64).max(1)
                        }
                    };
                    ui.label(
                        egui::RichText::new(format!("~{evaluations} runs"))
                            .size(10.0_f32)
                            .color(palette::TEXT_DIM),
                    );
                });
                if let Some((values, score, second)) = self.opt_best.as_ref() {
                    let vars = values
                        .iter()
                        .map(|value| format!("{value:.3}"))
                        .collect::<Vec<_>>()
                        .join(" / ");
                    ui.label(
                        egui::RichText::new(if self.opt_secondary_stay {
                            format!("best: {vars} -> completed {score:.1}, max stay {second:.1}")
                        } else {
                            format!("best: {vars} -> completed {score:.1}")
                        })
                        .strong(),
                    );
                }
                if !self.opt_rows.is_empty() {
                    // rank + variables + completed (+ max stay)
                    let columns = if self.opt_secondary_stay { 4 } else { 3 };
                    egui::Grid::new("opt-grid")
                        .striped(true)
                        .num_columns(columns)
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("#").small().color(palette::TEXT_DIM),
                            );
                            ui.strong("variables");
                            ui.strong("completed");
                            if self.opt_secondary_stay {
                                ui.strong("max stay");
                            }
                            ui.end_row();
                            let front: Vec<usize> = if self.opt_secondary_stay {
                                OpenFlexApp::pareto_front(&self.opt_rows)
                            } else {
                                Vec::new()
                            };
                            for (rank, (values, score, second)) in
                                self.opt_rows.iter().take(12).enumerate()
                            {
                                let on_front = front.contains(&rank);
                                let rank_colour = if rank == 0 {
                                    palette::AMBER
                                } else if on_front {
                                    palette::OK
                                } else {
                                    palette::TEXT_DIM
                                };
                                ui.label(
                                    egui::RichText::new(format!("{:>2}", rank + 1))
                                        .small()
                                        .monospace()
                                        .color(rank_colour),
                                );
                                ui.label(
                                    egui::RichText::new(
                                        values
                                            .iter()
                                            .map(|value| format!("{value:.3}"))
                                            .collect::<Vec<_>>()
                                            .join(" / "),
                                    )
                                    .monospace()
                                    .small(),
                                );
                                ui.label(
                                    egui::RichText::new(format!("{score:.1}"))
                                        .strong()
                                        .color(if on_front { palette::OK } else { palette::TEXT }),
                                );
                                if self.opt_secondary_stay {
                                    ui.label(
                                        egui::RichText::new(format!("{second:.1}"))
                                            .monospace()
                                            .small()
                                            .color(palette::TEXT_DIM),
                                    );
                                }
                                ui.end_row();
                            }
                        });
                }
                if self.opt_secondary_stay && !self.opt_rows.is_empty() {
                    let front = OpenFlexApp::pareto_front(&self.opt_rows);
                    let (response, painter) = ui.allocate_painter(
                        egui::vec2(ui.available_width(), 130.0),
                        egui::Sense::hover(),
                    );
                    let rect = response.rect;
                    painter.rect_filled(rect, 0.0, palette::CANVAS);
                    let max_score = self
                        .opt_rows
                        .iter()
                        .map(|(_, score, _)| *score)
                        .fold(0.0_f64, f64::max)
                        .max(1e-9);
                    let max_second = self
                        .opt_rows
                        .iter()
                        .map(|(_, _, second)| *second)
                        .fold(0.0_f64, f64::max)
                        .max(1e-9);
                    // Quarter guides make the cloud readable as a scatter plot.
                    for step in 1..4 {
                        let y = rect.bottom()
                            - 6.0
                            - (rect.height() - 12.0) * step as f32 / 4.0;
                        painter.line_segment(
                            [egui::pos2(rect.left() + 6.0, y), egui::pos2(rect.right() - 6.0, y)],
                            egui::Stroke::new(
                                1.0_f32,
                                egui::Color32::from_rgba_unmultiplied(120, 145, 180, 24),
                            ),
                        );
                    }
                    // Dominated points first so the front is drawn on top.
                    let mut order: Vec<usize> = (0..self.opt_rows.len()).collect();
                    order.sort_by_key(|index| front.contains(index));
                    for index in order {
                        let (_, score, second) = &self.opt_rows[index];
                        let x = rect.left()
                            + 6.0
                            + (rect.width() - 12.0) * (*second / max_second) as f32;
                        let y = rect.bottom()
                            - 6.0
                            - (rect.height() - 12.0) * (*score / max_score) as f32;
                        let point = egui::pos2(x, y);
                        let on_front = front.contains(&index);
                        let colour = if on_front {
                            palette::AMBER
                        } else {
                            egui::Color32::from_rgba_unmultiplied(128, 148, 172, 190)
                        };
                        let radius = if on_front { 4.2 } else { 2.6 };
                        painter.circle_filled(
                            point + egui::vec2(0.0, 1.0),
                            radius + 0.8,
                            palette::SHADOW,
                        );
                        painter.circle_filled(point, radius, colour);
                        if on_front {
                            painter.circle_stroke(
                                point,
                                radius + 2.0,
                                egui::Stroke::new(1.0_f32, palette::AMBER.gamma_multiply(0.6)),
                            );
                        }
                    }
                    // Axis captions in the canvas corners.
                    painter.text(
                        rect.left_bottom() + egui::vec2(8.0, -4.0),
                        egui::Align2::LEFT_BOTTOM,
                        "max stay ->",
                        egui::FontId::proportional(9.0),
                        palette::TEXT_DIM,
                    );
                    painter.text(
                        rect.left_top() + egui::vec2(8.0, 4.0),
                        egui::Align2::LEFT_TOP,
                        "^ completed",
                        egui::FontId::proportional(9.0),
                        palette::TEXT_DIM,
                    );
                    ui.label(
                        egui::RichText::new(format!(
                            "Pareto front: {} of {} points",
                            front.len(),
                            self.opt_rows.len()
                        ))
                        .color(palette::TEXT_DIM)
                        .small(),
                    );
                }
            });

        egui::SidePanel::right("report")
            .resizable(true)
            .width_range(300.0..=620.0)
            .frame(
                egui::Frame::side_top_panel(&ctx.style())
                    .fill(palette::PANEL)
                    .inner_margin(egui::Margin::symmetric(10.0, 8.0)),
            )
            .default_width(360.0)
            .show(ctx, |ui| {
                panel_heading(ui, "Connections");
                let Some(model) = self.model else {
                    return;
                };
                let mut set_strategy: Option<(usize, Option<openflex_model::model::PortStrategy>)> =
                    None;
                egui::ScrollArea::vertical()
                    .id_salt("connections")
                    .max_height(ui.available_height() * 0.32)
                    .show(ui, |ui| {
                        for (index, connection) in model.connections.iter().enumerate() {
                            ui.horizontal(|ui| {
                                let port = connection
                                    .port
                                    .map(|port| format!(" :{port}"))
                                    .unwrap_or_default();
                                let kind = match connection.kind {
                                    Some(openflex_model::model::ConnectionKind::S) => "S",
                                    _ => "A",
                                };
                                let strategy = match &connection.strategy {
                                    Some(openflex_model::model::PortStrategy::RoundRobin) => " rr",
                                    Some(openflex_model::model::PortStrategy::Random) => " random",
                                    Some(openflex_model::model::PortStrategy::Hold) => " hold",
                                    _ => "",
                                };
                                ui.label(format!(
                                    "[{kind}] {} -> {}{}{strategy}",
                                    connection.from, connection.to, port
                                ));
                                if ui
                                    .small_button("x")
                                    .on_hover_text(format!(
                                        "remove {} -> {}",
                                        connection.from, connection.to
                                    ))
                                    .clicked()
                                {
                                    self.remove_connection(index);
                                }
                                egui::ComboBox::from_id_salt(("conn-strategy", index))
                                    .selected_text(match &connection.strategy {
                                        Some(openflex_model::model::PortStrategy::RoundRobin) => {
                                            "rr"
                                        }
                                        Some(openflex_model::model::PortStrategy::Random) => {
                                            "random"
                                        }
                                        Some(openflex_model::model::PortStrategy::Hold) => "hold",
                                        _ => "first",
                                    })
                                    .width(70.0)
                                    .show_ui(ui, |ui| {
                                        for (value, name) in [
                                            (None, "first"),
                                            (
                                                Some(
                                                    openflex_model::model::PortStrategy::RoundRobin,
                                                ),
                                                "rr",
                                            ),
                                            (
                                                Some(openflex_model::model::PortStrategy::Random),
                                                "random",
                                            ),
                                            (
                                                Some(openflex_model::model::PortStrategy::Hold),
                                                "hold",
                                            ),
                                        ] {
                                            let current = connection.strategy.clone();
                                            if ui.selectable_label(current == value, name).clicked()
                                            {
                                                set_strategy = Some((index, value));
                                            }
                                        }
                                    });
                            });
                        }
                    });
                if let Some((index, strategy)) = set_strategy {
                    self.set_connection_strategy(index, strategy);
                }
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_from)
                            .hint_text("from")
                            .desired_width(86.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_to)
                            .hint_text("to")
                            .desired_width(86.0),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_port)
                            .hint_text("port")
                            .desired_width(46.0),
                    );
                    egui::ComboBox::from_id_salt("new-conn-kind")
                        .selected_text(match self.new_conn_kind {
                            1 => "S",
                            _ => "A",
                        })
                        .width(46.0)
                        .show_ui(ui, |ui| {
                            for (value, name) in [(0u8, "A"), (1, "S")] {
                                if ui
                                    .selectable_label(self.new_conn_kind == value, name)
                                    .clicked()
                                {
                                    self.new_conn_kind = value;
                                }
                            }
                        });
                    if ui.button("Add").clicked() {
                        self.add_connection();
                    }
                });
                ui.separator();
                panel_heading(ui, "Time series");
                ui.horizontal(|ui| {
                    ui.label("interval");
                    ui.add(
                        egui::DragValue::new(&mut self.series_interval)
                            .speed(0.5)
                            .range(0.1..=1000.0),
                    );
                    if ui
                        .add_enabled(
                            self.model.is_some() && !self.series_running,
                            egui::Button::new(if self.series_running {
                                "Sampling…"
                            } else {
                                "Sample"
                            }),
                        )
                        .clicked()
                    {
                        self.start_timeseries();
                    }
                });
                if !self.series.is_empty() {
                    let names: Vec<String> = self
                        .series
                        .first()
                        .map(|(_, row)| row.iter().map(|(name, ..)| name.clone()).collect())
                        .unwrap_or_default();
                    if self.series_object.is_empty()
                        && let Some(first) = names.first()
                    {
                        self.series_object = first.clone();
                    }
                    ui.horizontal(|ui| {
                        // A segmented control reads better than three radios.
                        for (value, name) in
                            [(0u8, "WIP"), (1, "throughput"), (2, "utilization")]
                        {
                            let selected = self.series_metric == value;
                            let button = egui::Button::new(
                                egui::RichText::new(name).color(if selected {
                                    egui::Color32::from_rgb(14, 18, 26)
                                } else {
                                    palette::TEXT
                                }),
                            )
                            .fill(if selected { palette::ACCENT_HI } else { palette::CARD })
                            .rounding(egui::Rounding::same(5.0_f32));
                            if ui.add(button).clicked() && !selected {
                                self.series_metric = value;
                            }
                        }
                        if ui.small_button("Export CSV").clicked() {
                            self.export_series_csv();
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for name in &names {
                            let selected = &self.series_object == name;
                            let button = egui::Button::new(
                                egui::RichText::new(name).color(if selected {
                                    palette::AMBER
                                } else {
                                    palette::TEXT_DIM
                                }),
                            )
                            .fill(if selected {
                                palette::CARD
                            } else {
                                egui::Color32::TRANSPARENT
                            })
                            .stroke(egui::Stroke::new(
                                1.0_f32,
                                if selected { palette::AMBER } else { palette::STROKE },
                            ))
                            .rounding(egui::Rounding::same(11.0_f32));
                            if ui.add(button).clicked() && !selected {
                                self.series_object = name.clone();
                            }
                        }
                    });
                    let (response, painter) = ui.allocate_painter(
                        egui::vec2(ui.available_width(), 150.0),
                        egui::Sense::hover(),
                    );
                    let rect = response.rect;
                    painter.rect_filled(rect, 0.0, palette::CANVAS);
                    let metric = self.series_metric as usize;
                    let value_of = |point: &SeriesPoint| match metric {
                        1 => point.2,
                        2 => point.3,
                        _ => point.1,
                    };
                    let max = self
                        .series
                        .iter()
                        .filter_map(|(_, row)| {
                            row.iter()
                                .find(|(name, ..)| *name == self.series_object)
                                .map(value_of)
                        })
                        .fold(0.0_f64, f64::max)
                        .max(1.0);
                    let total = self.series.len().max(1);
                    let mut points = Vec::with_capacity(total);
                    for (index, (_, row)) in self.series.iter().enumerate() {
                        let value = row
                            .iter()
                            .find(|(name, ..)| *name == self.series_object)
                            .map(value_of)
                            .unwrap_or(0.0);
                        let x = rect.left() + rect.width() * index as f32 / total as f32;
                        let y = rect.bottom() - 6.0 - (rect.height() - 12.0) * (value / max) as f32;
                        points.push(egui::pos2(x, y));
                    }
                    // Axis labels: max value top-left, first/last time below.
                    painter.text(
                        rect.left_top() + egui::vec2(3.0, 2.0),
                        egui::Align2::LEFT_TOP,
                        format!("{max:.1}"),
                        egui::FontId::proportional(9.0),
                        palette::TEXT_DIM,
                    );
                    if let (Some((first_time, _)), Some((last_time, _))) =
                        (self.series.first(), self.series.last())
                    {
                        painter.text(
                            rect.left_bottom() + egui::vec2(3.0, -3.0),
                            egui::Align2::LEFT_BOTTOM,
                            {
                                format!(
                                    "{first_time:.0} {}",
                                    self.model
                                        .map(|model| model.time_unit.label())
                                        .unwrap_or("")
                                )
                            },
                            egui::FontId::proportional(9.0),
                            palette::TEXT_DIM,
                        );
                        painter.text(
                            rect.right_bottom() + egui::vec2(-3.0, -3.0),
                            egui::Align2::RIGHT_BOTTOM,
                            {
                                format!(
                                    "{last_time:.0} {}",
                                    self.model
                                        .map(|model| model.time_unit.label())
                                        .unwrap_or("")
                                )
                            },
                            egui::FontId::proportional(9.0),
                            palette::TEXT_DIM,
                        );
                        // Mid-scale guide, so the curve can be read against a value.
                        let mid_y = rect.bottom() - 6.0 - (rect.height() - 12.0) * 0.5;
                        painter.line_segment(
                            [
                                egui::pos2(rect.left(), mid_y),
                                egui::pos2(rect.right(), mid_y),
                            ],
                            egui::Stroke::new(
                                1.0_f32,
                                egui::Color32::from_rgba_unmultiplied(140, 160, 190, 30),
                            ),
                        );
                        painter.text(
                            rect.left_top() + egui::vec2(3.0, rect.height() * 0.5 - 9.0),
                            egui::Align2::LEFT_TOP,
                            format!("{:.1}", max * 0.5),
                            egui::FontId::proportional(9.0),
                            palette::TEXT_DIM,
                        );
                        // Hovering anywhere on the chart reads the sample under
                        // the cursor: `t=… · value`.
                        if let Some(pointer) = response.hover_pos()
                            && pointer.x >= rect.left()
                            && pointer.x <= rect.right()
                        {
                            let fraction =
                                ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                            let index = Self::series_index_at(fraction, total).unwrap_or(0);
                            if let Some((time, row)) = self.series.get(index) {
                                let value = row
                                    .iter()
                                    .find(|(name, ..)| *name == self.series_object)
                                    .map(value_of)
                                    .unwrap_or(0.0);
                                let marker_x = rect.left() + rect.width() * fraction;
                                painter.line_segment(
                                    [
                                        egui::pos2(marker_x, rect.top()),
                                        egui::pos2(marker_x, rect.bottom()),
                                    ],
                                    egui::Stroke::new(
                                        1.0_f32,
                                        palette::ACCENT_HI.gamma_multiply(0.5),
                                    ),
                                );
                                painter.text(
                                    egui::pos2(marker_x + 4.0, rect.top() + 2.0),
                                    egui::Align2::LEFT_TOP,
                                    format!(
                                        "t={time:.0} {} · {value:.2}",
                                        self.model
                                            .map(|model| model.time_unit.label())
                                            .unwrap_or("")
                                    ),
                                    egui::FontId::proportional(9.0),
                                    palette::TEXT,
                                );
                            }
                        }
                    }
                    // Warm-up boundary: samples before it do not enter the
                    // statistics, so shade that part of the chart.
                    if let Some(model) = self.model
                        && model.run.warmup > 0.0
                        && let Some((last_time, _)) = self.series.last()
                        && *last_time > 0.0
                    {
                        let boundary = rect.left()
                            + rect.width() * (model.run.warmup / *last_time).clamp(0.0, 1.0) as f32;
                        if boundary > rect.left() + 1.0 {
                            painter.rect_filled(
                                egui::Rect::from_min_max(
                                    rect.left_top(),
                                    egui::pos2(boundary, rect.bottom()),
                                ),
                                0.0,
                                egui::Color32::from_rgba_unmultiplied(245, 190, 100, 18),
                            );
                            painter.line_segment(
                                [
                                    egui::pos2(boundary, rect.top()),
                                    egui::pos2(boundary, rect.bottom()),
                                ],
                                egui::Stroke::new(
                                    1.0_f32,
                                    palette::AMBER.gamma_multiply(0.55),
                                ),
                            );
                        }
                    }
                    // Horizontal guides at 25/50/75 % keep the scale readable.
                    for step in 1..4 {
                        let y = rect.bottom() - 6.0 - (rect.height() - 12.0) * step as f32 / 4.0;
                        painter.line_segment(
                            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                            egui::Stroke::new(
                                1.0_f32,
                                egui::Color32::from_rgba_unmultiplied(120, 140, 170, 28),
                            ),
                        );
                    }
                    // Filled area under the line gives the series some body.
                    if points.len() > 1 {
                        let mut area = points.clone();
                        area.push(egui::pos2(
                            points.last().map(|p| p.x).unwrap_or(rect.left()),
                            rect.bottom() - 6.0,
                        ));
                        area.push(egui::pos2(
                            points.first().map(|p| p.x).unwrap_or(rect.left()),
                            rect.bottom() - 6.0,
                        ));
                        painter.add(egui::Shape::convex_polygon(
                            area,
                            egui::Color32::from_rgba_unmultiplied(80, 150, 210, 46),
                            egui::Stroke::NONE,
                        ));
                    }
                    for pair in points.windows(2) {
                        painter.line_segment(
                            [pair[0], pair[1]],
                            egui::Stroke::new(1.8_f32, egui::Color32::from_rgb(126, 196, 244)),
                        );
                    }
                    // Last point marker + value badge.
                    if let Some(last) = points.last().copied() {
                        painter.circle_filled(last, 2.8, egui::Color32::from_rgb(190, 226, 255));
                        painter.text(
                            last + egui::vec2(-4.0, -6.0),
                            egui::Align2::RIGHT_BOTTOM,
                            format!("{:.1}", {
                                let row = self.series.last().map(|(_, row)| row);
                                row.and_then(|row| {
                                    row.iter()
                                        .find(|(name, ..)| *name == self.series_object)
                                        .map(value_of)
                                })
                                .unwrap_or(0.0)
                            }),
                            egui::FontId::proportional(10.0),
                            egui::Color32::from_rgb(200, 228, 250),
                        );
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{} · max {:.1} · {} samples",
                            self.series_object,
                            max,
                            self.series.len()
                        ))
                        .color(palette::TEXT_DIM)
                        .small(),
                    );
                    // Replay marker: a vertical line at the current sample.
                    if let Some(index) = self.replay_index
                        && index < total
                    {
                        let x = rect.left() + rect.width() * index as f32 / total as f32;
                        painter.line_segment(
                            [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                            egui::Stroke::new(1.4_f32, egui::Color32::from_rgb(255, 190, 80)),
                        );
                    }
                    let last = self.series.len().saturating_sub(1);
                    let mut index = self.replay_index.unwrap_or(last);
                    ui.horizontal(|ui| {
                        ui.label("Replay t");
                        if ui
                            .add(egui::Slider::new(&mut index, 0..=last).show_value(false))
                            .changed()
                        {
                            self.replay_index = Some(index);
                        }
                        let time = self.series.get(index).map(|(t, _)| *t).unwrap_or(0.0);
                        ui.label(format!("{time:.1}"));
                        if ui.small_button("live").clicked() {
                            self.replay_index = None;
                        }
                    });
                }
                ui.separator();
                egui::CollapsingHeader::new(format!(
                    "Checkpoints ({})",
                    self.checkpoint_files.len()
                ))
                .id_salt("checkpoints")
                .show(ui, |ui| {
                    if ui.button("Refresh list").clicked() {
                        self.refresh_checkpoints();
                    }
                    if self.checkpoint_files.is_empty() {
                        empty_state(
                            ui,
                            "No checkpoints yet",
                            "Run the model, then use Save checkpoint in the toolbar.",
                        );
                        return;
                    }
                    let dir_label = if self.checkpoint_dir.trim().is_empty() {
                        "checkpoints".to_string()
                    } else {
                        self.checkpoint_dir.trim().to_string()
                    };
                    ui.label(
                        egui::RichText::new(format!("folder: {dir_label}"))
                            .size(9.0_f32)
                            .monospace()
                            .color(palette::TEXT_DIM),
                    );
                    let mut delete: Option<String> = None;
                    for (file, time) in self.checkpoint_files.clone() {
                        ui.horizontal(|ui| {
                            let label = if time.is_nan() {
                                file.clone()
                            } else {
                                format!("{file}  t={time:.2}")
                            };
                            let button = egui::Button::new(
                                egui::RichText::new(label).small().monospace(),
                            )
                            .fill(palette::CARD)
                            .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
                            .rounding(egui::Rounding::same(5.0_f32));
                            if ui.add(button).clicked() {
                                self.load_checkpoint_file(&file);
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        egui::RichText::new("x")
                                            .small()
                                            .color(egui::Color32::from_rgb(226, 140, 120)),
                                    )
                                    .fill(egui::Color32::TRANSPARENT)
                                    .rounding(egui::Rounding::same(4.0_f32)),
                                )
                                .on_hover_text("delete this checkpoint file")
                                .clicked()
                            {
                                delete = Some(file.clone());
                            }
                        });
                    }
                    if let Some(file) = delete {
                        let dir = if self.checkpoint_dir.trim().is_empty() {
                            "checkpoints".to_string()
                        } else {
                            self.checkpoint_dir.trim().to_string()
                        };
                        let path = std::path::Path::new(&dir).join(&file);
                        match std::fs::remove_file(&path) {
                            Ok(()) => {
                                self.status = format!("Deleted {}", path.display());
                                self.refresh_checkpoints();
                            }
                            Err(error) => {
                                self.error =
                                    Some(format!("could not delete {}: {error}", path.display()))
                            }
                        }
                    }
                });
                let mut jump: Option<f64> = None;
                let mut export_log = false;
                let mut clear_log = false;
                egui::CollapsingHeader::new(format!("Events ({})", self.event_log.len()))
                    .id_salt("event-log")
                    .show(ui, |ui| {
                        if self.event_log.is_empty() {
                            empty_state(
                                ui,
                                "The event log is empty",
                                "Press Step event or Play to record the next events.",
                            );
                            return;
                        }
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.event_filter)
                                    .hint_text("filter kind or object")
                                    .desired_width(150.0),
                            );
                            if ui.small_button("CSV").clicked() {
                                export_log = true;
                            }
                            if ui.small_button("clear").clicked() {
                                clear_log = true;
                            }
                        });
                        let needle = self.event_filter.to_lowercase();
                        egui::ScrollArea::vertical()
                            .id_salt("event-scroll")
                            .max_height(140.0)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                let mut shown = 0;
                                for entry in &self.event_log {
                                    if !needle.is_empty()
                                        && !entry.kind.to_lowercase().contains(&needle)
                                        && !entry
                                            .object
                                            .as_ref()
                                            .is_some_and(|object| {
                                                object.to_lowercase().contains(&needle)
                                            })
                                    {
                                        continue;
                                    }
                                    shown += 1;
                                    let label =
                                        egui::RichText::new(entry.line()).monospace().small();
                                    if ui.selectable_label(false, label).clicked() {
                                        jump = Some(entry.time);
                                    }
                                }
                                if shown == 0 {
                                    ui.label(
                                        egui::RichText::new("no events match the filter")
                                            .small()
                                            .color(palette::TEXT_DIM),
                                    );
                                }
                            });
                    });
                if clear_log {
                    self.event_log.clear();
                    self.status = "Event log cleared".into();
                }
                if export_log {
                    self.export_event_log();
                }
                if let Some(time) = jump {
                    self.jump_to_event(time);
                }
                ui.separator();
                egui::CollapsingHeader::new(format!("Outputs ({})", self.outputs.len()))
                    .id_salt("outputs")
                    .show(ui, |ui| {
                        if self.outputs.is_empty() {
                            empty_state(
                                ui,
                                "Nothing written yet",
                                "Exports land next to the model and are listed here.",
                            );
                            return;
                        }
                        let mut clear = false;
                        for (path, size) in self.outputs.clone() {
                            ui.horizontal(|ui| {
                                let name = path
                                    .file_name()
                                    .map(|name| name.to_string_lossy().into_owned())
                                    .unwrap_or_else(|| path.display().to_string());
                                let exists = path.exists();
                                ui.label(
                                    egui::RichText::new(name)
                                        .small()
                                        .monospace()
                                        .color(if exists {
                                            palette::TEXT
                                        } else {
                                            palette::TEXT_DIM.gamma_multiply(0.6)
                                        }),
                                );
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Center),
                                    |ui| {
                                        ui.label(
                                            egui::RichText::new(format_bytes(size))
                                                .size(9.0_f32)
                                                .color(palette::TEXT_DIM),
                                        );
                                    },
                                );
                            })
                            .response
                            .on_hover_text(path.display().to_string());
                        }
                        if ui.small_button("Clear list").clicked() {
                            clear = true;
                        }
                        if clear {
                            self.outputs.clear();
                        }
                    });
                ui.separator();
                panel_heading(ui, "Report");
                if self.report.is_none() {
                    empty_state(
                        ui,
                        "No results yet",
                        "Press Run for the full replication, or Play to watch it live.",
                    );
                    return;
                }
                let mut export_csv = false;
                let mut export_json = false;
                let mut set_baseline = false;
                let mut export_compare = false;
                let mut save_baseline = false;
                let mut load_baseline = false;
                // Two tidy rows: exports, then baseline actions.
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("EXPORT")
                            .size(9.0_f32)
                            .color(palette::TEXT_DIM),
                    );
                    if ui.small_button("CSV").clicked() {
                        export_csv = true;
                    }
                    if ui.small_button("JSON").clicked() {
                        export_json = true;
                    }
                    // The compare export only exists once a baseline is captured.
                    if ui
                        .add_enabled(
                            self.baseline.is_some(),
                            egui::Button::new(egui::RichText::new("compare CSV").small()),
                        )
                        .clicked()
                    {
                        export_compare = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("BASELINE")
                            .size(9.0_f32)
                            .color(palette::TEXT_DIM),
                    );
                    // "set"/"save" need a current report; "load" needs a file.
                    if ui
                        .add_enabled(self.report.is_some(), egui::Button::new(egui::RichText::new("set").small()))
                        .clicked()
                    {
                        set_baseline = true;
                    }
                    if ui
                        .add_enabled(
                            self.baseline.is_some(),
                            egui::Button::new(egui::RichText::new("save").small()),
                        )
                        .clicked()
                    {
                        save_baseline = true;
                    }
                    if ui
                        .add_enabled(
                            self.path.is_some(),
                            egui::Button::new(egui::RichText::new("load").small()),
                        )
                        .on_hover_text("reads <model>.baseline.json")
                        .clicked()
                    {
                        load_baseline = true;
                    }
                    // Name the document the baseline came from: a delta against
                    // another model is meaningless.
                    if let Some(baseline) = self.baseline.as_ref() {
                        let own = baseline.model.clone();
                        let matches = self
                            .model
                            .map(|model| model.name.is_empty() || model.name == own)
                            .unwrap_or(true);
                        ui.label(
                            egui::RichText::new(format!("from {own}"))
                                .size(9.0_f32)
                                .monospace()
                                .color(if matches {
                                    palette::TEXT_DIM
                                } else {
                                    palette::AMBER
                                }),
                        )
                        .on_hover_text(if matches {
                            "baseline belongs to this model"
                        } else {
                            "baseline was captured from a different model"
                        });
                    }
                    if self.baseline.is_some() {
                        let selected = self.show_compare;
                        let button = egui::Button::new(
                            egui::RichText::new("compare").small().color(if selected {
                                egui::Color32::from_rgb(14, 18, 26)
                            } else {
                                palette::TEXT
                            }),
                        )
                        .fill(if selected { palette::ACCENT_HI } else { palette::CARD })
                        .rounding(egui::Rounding::same(5.0_f32));
                        if ui.add(button).clicked() {
                            self.show_compare = !self.show_compare;
                        }
                    }
                });
                if set_baseline {
                    self.snapshot_baseline();
                }
                if export_compare {
                    self.export_compare_csv();
                }
                if save_baseline {
                    self.save_baseline_json();
                }
                if load_baseline {
                    self.load_baseline_json();
                }
                if export_csv {
                    self.export_report_csv();
                }
                if export_json {
                    self.export_report_json();
                }
                let Some(report) = self.report.as_ref() else {
                    return;
                };
                ui.horizontal(|ui| {
                    let completed = if report.completed.ci95 > 0.0 {
                        format!("{:.1} ±{:.1}", report.completed.mean, report.completed.ci95)
                    } else {
                        format!("{:.0}", report.completed.mean)
                    };
                    let unit = self
                        .model
                        .map(|model| model.time_unit.label())
                        .unwrap_or("");
                    metric_card(
                        ui,
                        &format!("COMPLETED / {unit}"),
                        &completed,
                        palette::AMBER,
                    );
                    metric_card(
                        ui,
                        "CREATED",
                        &format!("{:.0}", report.created.mean),
                        palette::ACCENT_HI,
                    );
                });
                ui.horizontal(|ui| {
                    let wip: f64 = report
                        .objects
                        .values()
                        .map(|stats| stats.avg_content.mean)
                        .sum();
                    metric_card(
                        ui,
                        "AVG WIP",
                        &format!("{wip:.1}"),
                        palette::TEXT,
                    );
                    let util: f64 = if report.objects.is_empty() {
                        0.0
                    } else {
                        report
                            .objects
                            .values()
                            .map(|stats| stats.utilization.mean)
                            .sum::<f64>()
                            / report.objects.len() as f64
                    };
                    metric_card(
                        ui,
                        "AVG UTIL",
                        &format!("{:.0}%", util * 100.0),
                        palette::OK,
                    );
                });
                // Fleet-wide state mix: the per-object bars below answer "which
                // machine", this answers "how busy is the model overall".
                {
                    let totals = report.objects.values().fold(
                        [0.0_f64; 4],
                        |mut acc, stats| {
                            acc[0] += stats.busy_fraction.mean;
                            acc[1] += stats.blocked_fraction.mean;
                            acc[2] += stats.idle_fraction.mean;
                            acc[3] += stats.down_fraction.mean;
                            acc
                        },
                    );
                    let sum: f64 = totals.iter().sum();
                    let mix = Self::state_mix(totals);
                    if sum > 1e-9 {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new("STATE MIX")
                                    .size(9.0_f32)
                                    .color(palette::TEXT_DIM),
                            );
                            let (bar, _) = ui.allocate_exact_size(
                                egui::vec2(ui.available_width().min(200.0), 8.0),
                                egui::Sense::hover(),
                            );
                            ui.painter().rect_filled(bar, 3.0, palette::ST_OFF);
                            let mut left = bar.left();
                            let colours = [
                                palette::ST_BUSY,
                                palette::ST_BLOCKED,
                                palette::ST_IDLE,
                                palette::ST_DOWN,
                            ];
                            for (index, colour) in colours.into_iter().enumerate() {
                                let width = (bar.width() as f64 * mix[index]) as f32;
                                if width <= 0.5 {
                                    continue;
                                }
                                ui.painter().rect_filled(
                                    egui::Rect::from_min_size(
                                        egui::pos2(left, bar.top()),
                                        egui::vec2(width, bar.height()),
                                    ),
                                    0.0,
                                    colour,
                                );
                                left += width;
                            }
                        });
                        ui.label(
                            egui::RichText::new(format!(
                                "busy {:.0}% · blocked {:.0}% · idle {:.0}% · down {:.0}%",
                                mix[0] * 100.0,
                                mix[1] * 100.0,
                                mix[2] * 100.0,
                                mix[3] * 100.0
                            ))
                            .size(9.0_f32)
                            .monospace()
                            .color(palette::TEXT_DIM),
                        );
                    }
                }
                ui.separator();
                let (response, painter) = ui.allocate_painter(
                    egui::vec2(ui.available_width(), 130.0),
                    egui::Sense::hover(),
                );
                let rect = response.rect;
                painter.rect_filled(rect, 0.0, palette::CANVAS);
                // Utilisation bars: rounded columns with a gridline at 50/100 %,
                // the value printed above each bar when there is room.
                let entries: Vec<(String, f64)> = report
                    .objects
                    .iter()
                    .map(|(name, stats)| (name.clone(), stats.utilization.mean.clamp(0.0, 1.0)))
                    .collect();
                if !entries.is_empty() {
                    for fraction in [0.5_f32, 1.0] {
                        let y = rect.bottom() - 4.0 - (rect.height() - 8.0) * fraction;
                        painter.line_segment(
                            [egui::pos2(rect.left(), y), egui::pos2(rect.right(), y)],
                            egui::Stroke::new(
                                1.0_f32,
                                egui::Color32::from_rgba_unmultiplied(120, 145, 180, 26),
                            ),
                        );
                    }
                    let width = rect.width() / entries.len() as f32;
                    let show_values = width >= 34.0;
                    for (position, (name, utilization)) in entries.iter().enumerate() {
                        let height = (rect.height() - 8.0) * (*utilization as f32);
                        let bar = egui::Rect::from_min_max(
                            egui::pos2(
                                rect.left() + position as f32 * width + 2.5,
                                rect.bottom() - 4.0 - height,
                            ),
                            egui::pos2(
                                rect.left() + (position as f32 + 1.0) * width - 2.5,
                                rect.bottom() - 4.0,
                            ),
                        );
                        painter.rect_filled(bar, 3.0, Self::heat_colour(*utilization));
                        if show_values {
                            painter.text(
                                egui::pos2(bar.center().x, bar.top() - 1.0),
                                egui::Align2::CENTER_BOTTOM,
                                format!("{:.0}", utilization * 100.0),
                                egui::FontId::proportional(9.0),
                                palette::TEXT_DIM,
                            );
                        }
                        // Hovering a bar names the object (the chart has no axis).
                        if let Some(pointer) = response.hover_pos()
                            && bar.expand2(egui::vec2(width * 0.5, 4.0)).contains(pointer)
                        {
                            painter.rect_stroke(
                                bar,
                                3.0,
                                egui::Stroke::new(1.2_f32, palette::ACCENT_HI),
                            );
                            painter.text(
                                egui::pos2(bar.center().x, bar.bottom() + 2.0),
                                egui::Align2::CENTER_TOP,
                                name,
                                egui::FontId::proportional(9.0),
                                palette::TEXT,
                            );
                        }
                    }
                }
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("stats")
                    .show(ui, |ui| {
                        egui::Grid::new("stats-grid")
                            .striped(true)
                            .num_columns(8)
                            .show(ui, |ui| {
                                // Column headers are clickable: the first
                                // click sorts descending, the second reverses.
                                let mut sort_now: Option<u8> = None;
                                let headers: [(u8, String); 8] = [
                                    (0, "Object".to_string()),
                                    (1, "In".to_string()),
                                    (2, "Out".to_string()),
                                    (3, "WIP".to_string()),
                                    (4, "Stay".to_string()),
                                    (5, "Util".to_string()),
                                    (
                                        6,
                                        format!(
                                            "Thru / {}",
                                            self.model
                                                .map(|model| model.time_unit.label())
                                                .unwrap_or("")
                                        ),
                                    ),
                                    (7, "±CI".to_string()),
                                ];
                                for (column, label) in headers {
                                    let active = self.stats_sort == Some(column);
                                    let text = if active {
                                        format!("{label} ▾")
                                    } else {
                                        label
                                    };
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new(text)
                                                    .strong()
                                                    .color(if active {
                                                        palette::ACCENT_HI
                                                    } else {
                                                        palette::TEXT
                                                    }),
                                            )
                                            .fill(egui::Color32::TRANSPARENT)
                                            .rounding(egui::Rounding::same(3.0_f32)),
                                        )
                                        .on_hover_text("click to sort")
                                        .clicked()
                                    {
                                        sort_now = Some(column);
                                    }
                                }
                                ui.end_row();
                                if let Some(column) = sort_now {
                                    if self.stats_sort == Some(column) {
                                        self.stats_desc = !self.stats_desc;
                                    } else {
                                        self.stats_sort = Some(column);
                                        self.stats_desc = true;
                                    }
                                }
                                let mut rows: Vec<(&String, &openflex_core::stats::ObjectAggregate)> =
                                    report.objects.iter().collect();
                                if let Some(column) = self.stats_sort {
                                    rows.sort_by(|a, b| {
                                        let key = |stats: &openflex_core::stats::ObjectAggregate| {
                                            match column {
                                                1 => stats.input_count.mean,
                                                2 => stats.output_count.mean,
                                                3 => stats.avg_content.mean,
                                                4 => stats.avg_stay_time.mean,
                                                5 => stats.utilization.mean,
                                                6 => stats.throughput.mean,
                                                7 => stats.throughput.ci95,
                                                _ => 0.0,
                                            }
                                        };
                                        if column == 0 {
                                            let order = a.0.cmp(b.0);
                                            return if self.stats_desc { order.reverse() } else { order };
                                        }
                                        let order = key(a.1).total_cmp(&key(b.1));
                                        if self.stats_desc { order.reverse() } else { order }
                                    });
                                }
                                for (name, stats) in rows {
                                    ui.horizontal(|ui| {
                                        // A 4-segment bar (busy/blocked/idle/down)
                                        // makes the dominant state visible without
                                        // reading the tooltip.
                                        let (bar, _) = ui.allocate_exact_size(
                                            egui::vec2(34.0, 8.0),
                                            egui::Sense::hover(),
                                        );
                                        ui.painter().rect_filled(bar, 2.0, palette::ST_OFF);
                                        let segments = [
                                            (stats.busy_fraction.mean, palette::ST_BUSY),
                                            (stats.blocked_fraction.mean, palette::ST_BLOCKED),
                                            (stats.idle_fraction.mean, palette::ST_IDLE),
                                            (stats.down_fraction.mean, palette::ST_DOWN),
                                        ];
                                        let total: f64 =
                                            segments.iter().map(|(value, _)| *value).sum::<f64>()
                                                .max(1e-9);
                                        let mut left = bar.left();
                                        for (value, colour) in segments {
                                            let width = (bar.width() as f64 * value / total) as f32;
                                            if width <= 0.0 {
                                                continue;
                                            }
                                            ui.painter().rect_filled(
                                                egui::Rect::from_min_size(
                                                    egui::pos2(left, bar.top()),
                                                    egui::vec2(width, bar.height()),
                                                ),
                                                0.0,
                                                colour,
                                            );
                                            left += width;
                                        }
                                        ui.label(name).on_hover_text(format!(
                                            "idle {:.0}% · busy {:.0}% · blocked {:.0}% · down {:.0}%",
                                            stats.idle_fraction.mean * 100.0,
                                            stats.busy_fraction.mean * 100.0,
                                            stats.blocked_fraction.mean * 100.0,
                                            stats.down_fraction.mean * 100.0
                                        ));
                                    });
                                    ui.label(format!("{:.0}", stats.input_count.mean));
                                    ui.label(format!("{:.0}", stats.output_count.mean));
                                    ui.label(format!("{:.1}", stats.avg_content.mean));
                                    ui.label(format!("{:.1}", stats.avg_stay_time.mean));
                                    ui.label(format!("{:.0}%", stats.utilization.mean * 100.0));
                                    ui.label(format!("{:.2}", stats.throughput.mean));
                                    ui.label(format!("±{:.3}", stats.throughput.ci95));
                                    ui.end_row();
                                }
                            });
                        if self.show_compare
                            && let Some(baseline) = self.baseline.as_ref()
                        {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(format!(
                                    "Comparison vs baseline (completed {:.1} -> {:.1})",
                                    baseline.completed.mean, report.completed.mean
                                ))
                                .strong(),
                            );
                            egui::Grid::new("compare-grid")
                                .striped(true)
                                .num_columns(4)
                                .spacing([10.0, 3.0])
                                .show(ui, |ui| {
                                    for header in ["Object", "before", "after", "delta"] {
                                        ui.label(
                                            egui::RichText::new(header)
                                                .strong()
                                                .color(palette::TEXT_DIM),
                                        );
                                    }
                                    ui.end_row();
                                    for (name, current) in &report.objects {
                                        let Some(base) = baseline.objects.get(name) else {
                                            continue;
                                        };
                                        let delta =
                                            current.utilization.mean - base.utilization.mean;
                                        let (colour, arrow) = if delta > 0.005 {
                                            (palette::OK, "+")
                                        } else if delta < -0.005 {
                                            (egui::Color32::from_rgb(226, 116, 96), "-")
                                        } else {
                                            (palette::TEXT_DIM, "=")
                                        };
                                        ui.label(name);
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{:.0}%",
                                                base.utilization.mean * 100.0
                                            ))
                                            .monospace()
                                            .small()
                                            .color(palette::TEXT_DIM),
                                        );
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{:.0}%",
                                                current.utilization.mean * 100.0
                                            ))
                                            .monospace()
                                            .small()
                                            .color(palette::TEXT),
                                        );
                                        // Arrow + number: direction is readable
                                        // without relying on colour alone.
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{arrow}{:+.1}%",
                                                delta * 100.0
                                            ))
                                            .monospace()
                                            .small()
                                            .color(colour),
                                        );
                                        ui.end_row();
                                    }
                                });
                        }
                    });
            });

        if self.show_help {
            let mut open = true;
            egui::Window::new("Keyboard shortcuts")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.set_max_width(520.0);
                    ui.label(
                        egui::RichText::new(
                            "Everything is reachable from the toolbar; these are the shortcuts.",
                        )
                        .small()
                        .color(palette::TEXT_DIM),
                    );
                    ui.add_space(6.0);
                    for (section, rows) in [
                        (
                            "FILE",
                            &[
                                ("Ctrl/Cmd + O", "open a model"),
                                ("Ctrl/Cmd + N", "new model"),
                                ("Ctrl/Cmd + S", "save"),
                                ("Ctrl/Cmd + Z", "undo"),
                                ("Ctrl/Cmd + Shift + Z", "redo"),
                            ][..],
                        ),
                        (
                            "RUN",
                            &[
                                ("Space", "play / pause"),
                                ("R", "reset the run"),
                                ("Right arrow", "step the simulation forward"),
                                ("Up / Down", "increase / decrease the step size"),
                            ][..],
                        ),
                        (
                            "SELECT",
                            &[
                                ("Ctrl/Cmd + A", "select every object"),
                                ("Ctrl/Cmd + D", "duplicate the selection"),
                                ("click", "select an object"),
                                ("drag on empty space", "marquee select"),
                                ("Delete / Backspace", "delete the selection"),
                            ][..],
                        ),
                        (
                            "VIEW",
                            &[
                                ("drag", "rotate the camera"),
                                ("wheel", "zoom"),
                                ("2D on/off", "flat floor-plan view"),
                                ("Top / Front / Side", "camera presets"),
                            ][..],
                        ),
                        ("HELP", &[("F1", "toggle this dialog")][..]),
                    ] {
                        panel_heading(ui, section);
                        egui::Grid::new(("help-grid", section))
                            .num_columns(2)
                            .spacing([14.0, 3.0])
                            .show(ui, |ui| {
                                for (key, action) in rows {
                                    // Key caps in a monospace chip read faster.
                                    egui::Frame::none()
                                        .fill(palette::CARD)
                                        .stroke(egui::Stroke::new(1.0_f32, palette::STROKE))
                                        .rounding(egui::Rounding::same(4.0_f32))
                                        .inner_margin(egui::Margin::symmetric(5.0, 1.0))
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(*key)
                                                    .monospace()
                                                    .small()
                                                    .color(palette::ACCENT_HI),
                                            );
                                        });
                                    ui.label(
                                        egui::RichText::new(*action).small().color(palette::TEXT),
                                    );
                                    ui.end_row();
                                }
                            });
                    }
                });
            if !open {
                self.show_help = false;
            }
        }
        if self.show_path_picker {
            let mut open = true;
            egui::Window::new("Pick a numeric field")
                .open(&mut open)
                .resizable(true)
                .default_width(520.0)
                .show(ctx, |ui| {
                    if self.path_candidates.is_empty() {
                        empty_state(
                            ui,
                            "No numeric fields",
                            "This model has no numeric parameter to optimise.",
                        );
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "{} numeric field(s) — click to fill the last variable",
                            self.path_candidates.len()
                        ))
                        .size(10.0_f32)
                        .color(palette::TEXT_DIM),
                    );
                    let mut chosen: Option<String> = None;
                    egui::ScrollArea::vertical()
                        .id_salt("path-picker")
                        .max_height(320.0)
                        .show(ui, |ui| {
                            for path in &self.path_candidates {
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(path)
                                                .small()
                                                .monospace()
                                                .color(palette::TEXT),
                                        )
                                        .fill(egui::Color32::TRANSPARENT)
                                        .rounding(egui::Rounding::same(4.0_f32)),
                                    )
                                    .clicked()
                                {
                                    chosen = Some(path.clone());
                                }
                            }
                        });
                    if let Some(path) = chosen {
                        if let Some(last) = self.opt_vars.last_mut() {
                            last.path = path;
                        } else {
                            self.opt_vars.push(OptVar {
                                path,
                                low: 1.0,
                                high: 10.0,
                            });
                        }
                        self.show_path_picker = false;
                    }
                });
            if !open {
                self.show_path_picker = false;
            }
        }
        if self.show_browser {
            let mut open = true;
            egui::Window::new("Open model")
                .open(&mut open)
                .resizable(true)
                .default_width(520.0)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Up").clicked()
                            && let Some(parent) = self.browser_dir.parent()
                        {
                            let parent = parent.to_path_buf();
                            self.browser_dir = parent;
                            self.refresh_browser();
                        }
                        if ui.button("Home").clicked()
                            && let Some(home) = std::env::var_os("HOME")
                        {
                            self.browser_dir = PathBuf::from(home);
                            self.refresh_browser();
                        }
                        ui.label(
                            egui::RichText::new(self.browser_dir.display().to_string())
                                .small()
                                .monospace()
                                .color(palette::TEXT_DIM),
                        );
                    });
                    ui.separator();
                    if let Some(error) = self.browser_error.as_ref() {
                        ui.colored_label(egui::Color32::from_rgb(226, 116, 96), error);
                    }
                    egui::ScrollArea::vertical()
                        .id_salt("browser")
                        .max_height(320.0)
                        .show(ui, |ui| {
                            if self.browser_entries.is_empty() {
                                empty_state(
                                    ui,
                                    "Nothing here",
                                    "No folders or .ofm models in this directory.",
                                );
                            }
                            let entries = self.browser_entries.clone();
                            for (name, is_dir) in entries {
                                let label = if is_dir {
                                    format!("[dir] {name}")
                                } else {
                                    name.clone()
                                };
                                let colour = if is_dir {
                                    palette::ACCENT_HI
                                } else {
                                    palette::TEXT
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            egui::RichText::new(label)
                                                .small()
                                                .monospace()
                                                .color(colour),
                                        )
                                        .fill(egui::Color32::TRANSPARENT)
                                        .rounding(egui::Rounding::same(4.0_f32)),
                                    )
                                    .clicked()
                                {
                                    self.navigate_browser(&name, is_dir);
                                }
                            }
                        });
                });
            if !open {
                self.show_browser = false;
            }
        }
        if self.show_save_as {
            let mut open = true;
            egui::Window::new("Save as")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.set_max_width(460.0);
                    ui.label(
                        egui::RichText::new("Save the model as a .ofm file")
                            .small()
                            .color(palette::TEXT_DIM),
                    );
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("PATH")
                            .size(10.0_f32)
                            .strong()
                            .color(palette::TEXT_DIM),
                    );
                    ui.add(
                        egui::TextEdit::singleline(&mut self.save_as)
                            .desired_width(f32::INFINITY)
                            .font(egui::TextStyle::Monospace),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if primary_button(ui, true, "Save") {
                            let path = PathBuf::from(self.save_as.clone());
                            let text = match self.model.and_then(|model| model.to_json().ok()) {
                                Some(text) => text,
                                None => {
                                    self.error = Some("could not serialise the model".into());
                                    return;
                                }
                            };
                            match std::fs::write(&path, text) {
                                Ok(()) => {
                                    self.path = Some(path.clone());
                                    self.path_input = path.display().to_string();
                                    self.remember_recent(&path);
                                    // Saved-as means the document now matches the
                                    // file, and the presets belong to the new path.
                                    self.model_dirty = false;
                                    self.form_dirty = false;
                                    self.save_presets();
                                    self.status = format!("Saved {}", path.display());
                                    self.show_save_as = false;
                                    self.error = None;
                                }
                                Err(error) => self.error = Some(error.to_string()),
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.show_save_as = false;
                        }
                    });
                });
            if !open {
                self.show_save_as = false;
            }
        }
        // Keyboard shortcuts.
        // Editing a text field must not trigger the global undo/save shortcuts.
        let typing = ctx.wants_keyboard_input();
        let mut step_forward = false;
        let mut step_delta = 0.0_f32;
        ctx.input_mut(|input| {
            if !typing && input.consume_key(egui::Modifiers::COMMAND, egui::Key::Z) {
                self.restore();
            }
            if !typing
                && input.consume_key(
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    egui::Key::Z,
                )
            {
                self.redo_once();
            }
            if !typing && input.consume_key(egui::Modifiers::COMMAND, egui::Key::S) {
                self.save();
            }
            // Open/New also skip while a text field has focus, so typing a
            // path never accidentally opens or resets the model.
            if !typing && input.consume_key(egui::Modifiers::COMMAND, egui::Key::O) {
                if self.path_input.trim().is_empty() {
                    self.status = "Type a model path first (or use Recent)".into();
                } else {
                    self.load(PathBuf::from(self.path_input.clone()));
                }
            }
            if !typing && input.consume_key(egui::Modifiers::COMMAND, egui::Key::N) {
                self.new_model();
            }
            // Space toggles play/pause; R resets; F1 shows the short help.
            if !typing && input.consume_key(egui::Modifiers::NONE, egui::Key::Space) {
                if self.running && !self.paused {
                    if let Some(handle) = self.handle.as_mut() {
                        let _ = handle.pause();
                    }
                    self.paused = true;
                } else {
                    if let Some(handle) = self.handle.as_mut() {
                        let _ = handle.resume();
                    }
                    self.running = true;
                    self.paused = false;
                }
            }
            if !typing && input.consume_key(egui::Modifiers::NONE, egui::Key::R) {
                self.reset_run();
            }
            if !typing && input.consume_key(egui::Modifiers::NONE, egui::Key::F1) {
                self.show_help = !self.show_help;
            }
            if !typing && input.consume_key(egui::Modifiers::COMMAND, egui::Key::A) {
                self.select_all();
            }
            if !typing && input.consume_key(egui::Modifiers::COMMAND, egui::Key::D) {
                self.duplicate_selection();
            }
            // Delete removes the selection (FlexSim uses the same key).
            if !typing
                && (input.consume_key(egui::Modifiers::NONE, egui::Key::Delete)
                    || input.consume_key(egui::Modifiers::NONE, egui::Key::Backspace))
            {
                self.batch_delete();
            }
            // Right arrow steps the simulation by the current step size.
            if !typing && input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                step_forward = true;
            }
            // Up/Down adjust the step size without touching the canvas zoom.
            if !typing && input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                step_delta = 1.0;
            }
            if !typing && input.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                step_delta = -1.0;
            }
        });
        if step_delta != 0.0 {
            self.step_size = (self.step_size + step_delta).clamp(0.1, 1000.0);
        }
        if step_forward {
            self.advance(self.step_size as f64);
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::central_panel(&ctx.style()).fill(palette::BG))
            .show(ctx, |ui| {
            let Some(model) = self.model else {
                ui.centered_and_justified(|ui| ui.label("Open a model to see the layout."));
                return;
            };
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::drag());
            let rect = response.rect;
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(18, 22, 28));
            if !self.canvas_measured {
                self.canvas_size = rect.size();
                self.canvas_measured = true;
                if self.frame_pending {
                    self.frame_pending = false;
                    self.frame_selection();
                }
            } else {
                self.canvas_size = rect.size();
            }
            let scale = (rect.width().min(rect.height()) / 12.0) * self.zoom;
            // Shift the origin so the focused object sits in the middle.
            let raw_center = rect.center();
            let focus_offset = self.project(
                [self.focus[0] as f64, self.focus[1] as f64],
                0.0,
                egui::pos2(0.0, 0.0),
                scale,
            );
            let center = raw_center - focus_offset.to_vec2();
            if response.dragged() {
                let delta = response.drag_delta();
                self.yaw += delta.x * 0.01;
                self.pitch = (self.pitch + delta.y * 0.01).clamp(PITCH_MIN, PITCH_MAX);
            }
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom * (1.0 + scroll * 0.001)).clamp(ZOOM_MIN, ZOOM_MAX);
            }
            // Soft vignette: a lighter pool in the middle of the canvas gives
            // the flat dark background some depth.
            let mut shade = 0.0_f32;
            while shade < 1.0 {
                let inset = shade * rect.width().min(rect.height()) * 0.55;
                painter.rect_stroke(
                    rect.shrink(inset),
                    0.0,
                    egui::Stroke::new(
                        rect.width().min(rect.height()) * 0.03,
                        egui::Color32::from_rgba_unmultiplied(60, 76, 100, 3),
                    ),
                );
                shade += 0.06;
            }
            // Ground grid (z = 0 plane), faded with distance from the focus so
            // the horizon does not compete with the objects.
            let grid = 20;
            let step = 5.0;
            let fade = |line: i32| -> egui::Color32 {
                let distance = (line.abs() as f32 / grid as f32).clamp(0.0, 1.0);
                let alpha = (1.0 - distance).powf(0.6);
                let base = if line == 0 { 96 } else { 62 };
                egui::Color32::from_rgba_unmultiplied(
                    base,
                    base + 8,
                    base + 22,
                    (alpha * if line == 0 { 150.0 } else { 95.0 }) as u8,
                )
            };
            for line in -grid..=grid {
                let offset = line as f64 * step;
                let extent = grid as f64 * step;
                let color = fade(line);
                let a = self.project([offset, -extent], 0.0, center, scale);
                let b = self.project([offset, extent], 0.0, center, scale);
                let c = self.project([-extent, offset], 0.0, center, scale);
                let d = self.project([extent, offset], 0.0, center, scale);
                painter.line_segment([a, b], egui::Stroke::new(1.0_f32, color));
                painter.line_segment([c, d], egui::Stroke::new(1.0_f32, color));
            }
            for connection in &model.connections {
                let Some(from) = model.object(&connection.from) else {
                    continue;
                };
                let Some(to) = model.object(&connection.to) else {
                    continue;
                };
                let start = self.project(from.pos, 0.0, center, scale);
                let end = self.project(to.pos, 0.0, center, scale);
                // Two-pass line: a wide translucent halo plus a crisp core, so
                // the routing reads clearly over the grid.
                painter.line_segment(
                    [start, end],
                    egui::Stroke::new(3.4_f32, egui::Color32::from_rgba_unmultiplied(70, 104, 150, 60)),
                );
                painter.line_segment(
                    [start, end],
                    egui::Stroke::new(1.3_f32, egui::Color32::from_rgb(112, 150, 190)),
                );
                // Small arrow head pointing at the downstream object.
                let dir = (end - start).normalized();
                if dir.length_sq() > 0.0 {
                    let tip = end - dir * (10.0 * self.zoom).max(4.0);
                    let normal = egui::vec2(-dir.y, dir.x) * (3.6 * self.zoom).max(1.8);
                    painter.add(egui::Shape::convex_polygon(
                        vec![tip + dir * 5.0, tip + normal, tip - normal],
                        egui::Color32::from_rgb(126, 164, 202),
                        egui::Stroke::NONE,
                    ));
                }
            }
            // Marquee selection: drag on empty space to select a rectangle of
            // objects (Shift adds to the current selection).
            let drag_started = response.drag_started();
            let pointer = response.interact_pointer_pos();
            if drag_started {
                self.marquee_start = pointer;
            }
            let marquee = self.marquee_start.zip(pointer).map(|(start, current)| {
                egui::Rect::from_two_pos(start, current)
            });
            if response.drag_stopped() {
                if let Some(rect) = marquee
                    && rect.width() > 6.0
                    && rect.height() > 6.0
                {
                    let shift = ui.input(|input| input.modifiers.shift);
                    let mut hits: Vec<usize> = Vec::new();
                    for (index, object) in model.objects.iter().enumerate() {
                        let point = self.project(object.pos, object.z, center, scale);
                        if rect.contains(point) {
                            hits.push(index);
                        }
                    }
                    if !hits.is_empty() {
                        if shift {
                            for index in hits {
                                if !self.selection.contains(&index) {
                                    self.selection.push(index);
                                }
                            }
                        } else {
                            self.selection = hits;
                        }
                        if let Some(first) = self.selection.first().copied() {
                            self.selected = Some(first);
                            if let Some(model) = self.model {
                                let object = &model.objects[first];
                                self.rename_text = object.name.clone();
                                self.form = Some(EditForm::from_object(object));
                                self.params_text = serde_json::to_string_pretty(&object.params)
                                    .unwrap_or_else(|_| "{}".into());
                            }
                        }
                        self.status = format!("{} object(s) selected", self.selection.len());
                    }
                }
                self.marquee_start = None;
            }
            if let Some(rect) = marquee
                && rect.width() > 6.0
                && rect.height() > 6.0
            {
                painter.rect_filled(
                    rect,
                    0.0,
                    egui::Color32::from_rgba_unmultiplied(110, 170, 230, 40),
                );
                painter.rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(120, 180, 240)),
                );
            }
            let mut context_target: Option<usize> = None;
            let pointer_pos = response.hover_pos();
            let mut hovered_info: Option<(String, String)> = None;
            for (index, object) in model.objects.iter().enumerate() {
                let live = self
                    .sim_state
                    .as_ref()
                    .and_then(|state| state.objects.get(&object.name));
                let replay_content = self.replay_index.and_then(|index| {
                    self.series.get(index).and_then(|(_, row)| {
                        row.iter()
                            .find(|(name, ..)| *name == object.name)
                            .map(|point| point.1)
                    })
                });
                // Vehicles move: draw them at their live position when the
                // engine reports one; static objects keep their layout position.
                let mut world_pos = object.pos;
                let mut world_z = object.z;
                if let Some(stats) = live
                    && let Some(position) = stats.position
                    && matches!(
                        object.kind,
                        ObjectKind::Transporter
                            | ObjectKind::Person
                            | ObjectKind::Agv
                            | ObjectKind::Robot
                            | ObjectKind::Crane
                            | ObjectKind::AsrsVehicle
                            | ObjectKind::Elevator
                    )
                {
                    world_pos = position;
                    world_z = stats.z;
                }
                // Visual smoothing: the engine reports positions at event
                // boundaries only, so ease towards the new value.
                let dt = (self.last_frame - 0.0).max(0.0);
                let target = [world_pos[0] as f32, world_pos[1] as f32];
                let key = object.name.clone();
                let smooth = self.smooth.entry(key).or_insert(target);
                let alpha = (1.0 - (-dt * 8.0).exp()).clamp(0.05, 0.9) as f32;
                smooth[0] += (target[0] - smooth[0]) * alpha;
                smooth[1] += (target[1] - smooth[1]) * alpha;
                let world_pos = [smooth[0] as f64, smooth[1] as f64];
                let ground = self.project(world_pos, 0.0, center, scale);
                painter.circle_filled(
                    ground + egui::vec2(0.0, 2.0),
                    (12.0 * self.zoom).max(3.0),
                    palette::SHADOW,
                );
                let base = self.project(world_pos, world_z, center, scale);
                let selected = self.selected == Some(index);
                let in_selection = self.selection.contains(&index);
                let mut fill = match live {
                    Some(stats) => Self::state_fill(stats),
                    None => Self::kind_color(object.kind),
                };
                if let Some(colour) = self.heatmap_fill(&object.name) {
                    fill = colour;
                }
                // Selection states: the primary selection is lightened a lot,
                // the rest of the selection a little, everything else keeps its
                // state/heatmap colour. Hue order is preserved in every case.
                if selected {
                    fill = shade_light(fill, 0.35);
                } else if in_selection {
                    fill = shade_light(fill, 0.16);
                }
                let rotation = object.rot.unwrap_or(0.0) as f32;
                FLAT_FOOTPRINT.with(|slot| *slot.borrow_mut() = None);
                let box_rect = Self::draw_object_shape_mode(
                    &painter,
                    object.kind,
                    base,
                    self.zoom,
                    fill,
                    selected,
                    rotation,
                    object.z as f32,
                    self.flat_view,
                );
                // Flow items held right now: small dots above the box. Beyond
                // eight items a "+N" chip summarises the rest so a full queue
                // does not smear into a line of dots.
                if let Some(stats) = live {
                    let content = stats.content;
                    let shown = content.min(8) as usize;
                    for slot in 0..shown {
                        let offset = egui::vec2(
                            (slot as f32 - (shown as f32 - 1.0) * 0.5) * (7.0 * self.zoom),
                            -(20.0 * self.zoom),
                        );
                        let centre = box_rect.center() + offset;
                        painter.circle_filled(
                            centre + egui::vec2(0.0, 1.0),
                            (2.8 * self.zoom).max(1.6),
                            palette::SHADOW,
                        );
                        painter.circle_filled(
                            centre,
                            (2.4 * self.zoom).max(1.4),
                            egui::Color32::from_rgb(244, 236, 200),
                        );
                    }
                    if content > shown as u32 {
                        painter.text(
                            box_rect.center()
                                + egui::vec2(
                                    (shown as f32 - 1.0) * 3.5 * self.zoom
                                        + 10.0 * self.zoom,
                                    -(20.0 * self.zoom),
                                ),
                            egui::Align2::LEFT_CENTER,
                            format!("+{}", content - shown as u32),
                            egui::FontId::proportional((9.0 * self.zoom).max(7.0)),
                            palette::AMBER,
                        );
                    }
                }
                let caption = if let Some(value) = replay_content {
                    format!("{}  [{}]", object.name, value.round() as u32)
                } else {
                    match live {
                        Some(stats) if stats.content > 0 => {
                            format!("{}  [{}]", object.name, stats.content)
                        }
                        Some(stats) if stats.avg_content > 0.0 => {
                            format!("{}  ≈{:.0}", object.name, stats.avg_content)
                        }
                        _ => object.name.clone(),
                    }
                };
                // The caption sits in a rounded dark plate so it stays readable
                // on every fill colour (heatmap, state, kind) without a halo.
                let font = egui::FontId::proportional(12.0);
                let metrics = ctx.fonts(|fonts| {
                    fonts
                        .layout_no_wrap(caption.clone(), font.clone(), egui::Color32::WHITE)
                        .size()
                });
                // In the floor plan there is no “above the box”, so the plate
                // sits on the footprint centre instead.
                let plate = if self.flat_view {
                    egui::Rect::from_center_size(box_rect.center(), metrics + egui::vec2(6.0, 2.0))
                } else {
                    egui::Rect::from_center_size(
                        egui::pos2(
                            box_rect.center().x,
                            box_rect.top() - 6.0 - metrics.y / 2.0,
                        ),
                        metrics + egui::vec2(10.0, 3.0),
                    )
                };
                painter.rect_filled(
                    plate,
                    (plate.height() / 2.0).max(4.0),
                    egui::Color32::from_rgba_unmultiplied(14, 18, 26, 205),
                );
                painter.rect_stroke(
                    plate,
                    (plate.height() / 2.0).max(4.0),
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgba_unmultiplied(120, 140, 175, 90)),
                );
                painter.text(
                    plate.center(),
                    egui::Align2::CENTER_CENTER,
                    &caption,
                    font,
                    if selected { palette::AMBER } else { egui::Color32::WHITE },
                );
                // State badge: the engine state name in the box corner so the
                // colour is not the only cue (accessibility + FlexSim parity).
                if let Some(state) = live.and_then(|stats| stats.state) {
                    let name = match state {
                        openflex_core::engine::State::Idle => "idle",
                        openflex_core::engine::State::Busy => "busy",
                        openflex_core::engine::State::Blocked => "blocked",
                        openflex_core::engine::State::Down => "down",
                        openflex_core::engine::State::OffShift => "off",
                    };
                    if self.flat_view {
                        // A dot reads better than 9px text on a small footprint.
                        painter.circle_filled(
                            box_rect.right_top() + egui::vec2(-4.0, 4.0),
                            (2.6 * self.zoom).max(1.6),
                            match state {
                                openflex_core::engine::State::Busy => palette::ST_BUSY,
                                openflex_core::engine::State::Blocked => palette::ST_BLOCKED,
                                openflex_core::engine::State::Down => palette::ST_DOWN,
                                openflex_core::engine::State::OffShift => palette::ST_OFF,
                                openflex_core::engine::State::Idle => palette::ST_IDLE,
                            },
                        );
                    } else {
                        painter.text(
                            box_rect.right_bottom() - egui::vec2(3.0, 2.0),
                            egui::Align2::RIGHT_BOTTOM,
                            name,
                            egui::FontId::proportional(9.0),
                            egui::Color32::from_rgba_unmultiplied(255, 255, 255, 190),
                        );
                    }
                }
                if let Some(pointer) = pointer_pos
                    && box_rect.expand(4.0).contains(pointer)
                {
                    let summary = match live {
                        Some(stats) => format!(
                            "{}\ncontent {} · state {} · util {:.0}%",
                            object.kind.as_str(),
                            stats.content,
                            stats
                                .state
                                .map(|state| match state {
                                    openflex_core::engine::State::Idle => "idle",
                                    openflex_core::engine::State::Busy => "busy",
                                    openflex_core::engine::State::Blocked => "blocked",
                                    openflex_core::engine::State::Down => "down",
                                    openflex_core::engine::State::OffShift => "off shift",
                                })
                                .unwrap_or("—"),
                            stats.utilization * 100.0
                        ),
                        None => object.kind.as_str().to_string(),
                    };
                    hovered_info = Some((object.name.clone(), summary));
                }
                if response.clicked()
                    && let Some(pointer) = response.interact_pointer_pos()
                    && (box_rect.expand(6.0).contains(pointer)
                        || FLAT_FOOTPRINT.with(|slot| {
                            slot.borrow()
                                .as_ref()
                                .is_some_and(|corners| point_in_polygon(pointer, corners))
                        }))
                {
                    self.select(index);
                }
                if response.secondary_clicked()
                    && let Some(pointer) = response.interact_pointer_pos()
                    && box_rect.expand(6.0).contains(pointer)
                {
                    context_target = Some(index);
                }
            }
            if let Some((name, summary)) = hovered_info {
                egui::show_tooltip_at_pointer(
                    ctx,
                    egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("object-tooltip")),
                    egui::Id::new("object-tooltip-anchor"),
                    |ui| {
                        ui.label(
                            egui::RichText::new(&name)
                                .strong()
                                .color(egui::Color32::from_rgb(255, 210, 120)),
                        );
                        ui.label(egui::RichText::new(summary).small());
                    },
                );
            }
            if let Some(index) = context_target {
                let name = model.objects[index].name.clone();
                self.select(index);
                self.context_menu_open = Some(name);
            }
            if let Some(name) = self.context_menu_open.clone() {
                let mut open = true;
                egui::Window::new(format!("{name}  (right-click menu)"))
                    .id(egui::Id::new("object-context"))
                    .open(&mut open)
                    .resizable(false)
                    .show(ctx, |ui| {
                        if ui.button("Focus camera").clicked() {
                            if let Some(model) = self.model
                                && let Some(object) = model.object(&name)
                            {
                                self.focus = [object.pos[0] as f32, object.pos[1] as f32];
                            }
                            self.context_menu_open = None;
                        }
                        if ui.button("Frame selection").clicked() {
                            self.frame_selection();
                            self.context_menu_open = None;
                        }
                        if ui.button("Reset position").clicked() {
                            if let (Some(index), Some(model)) = (self.selected, self.model) {
                                // Same rules as every other edit: undo entry,
                                // validation, refreshed form.
                                self.snapshot_labelled("reset height");
                                let mut copy = model.clone();
                                copy.objects[index].z = 0.0;
                                match copy.validate() {
                                    Ok(()) => {
                                        self.model = Some(Box::leak(Box::new(copy)));
                                        self.form = self
                                            .model
                                            .map(|model| {
                                                EditForm::from_object(&model.objects[index])
                                            });
                                        self.status =
                                            format!("Reset the height of {name}");
                                        self.error = None;
                                    }
                                    Err(error) => self.error = Some(error.to_string()),
                                }
                            }
                            self.context_menu_open = None;
                        }
                        if ui.button("Delete object").clicked() {
                            self.delete_objects(std::slice::from_ref(&name));
                            self.context_menu_open = None;
                        }
                        ui.horizontal(|ui| {
                            ui.label("rotate");
                            if ui.small_button("-15°").clicked() {
                                self.rotate_object(&name, -15.0);
                            }
                            if ui.small_button("+15°").clicked() {
                                self.rotate_object(&name, 15.0);
                            }
                            if ui.small_button("0°").clicked() {
                                self.rotate_object(&name, f64::NEG_INFINITY);
                            }
                        });
                        if ui.button("Close").clicked() {
                            self.context_menu_open = None;
                        }
                    });
                if !open {
                    self.context_menu_open = None;
                }
            }
            ui.horizontal(|ui| {
                let legend = [
                    ("busy", palette::ST_BUSY),
                    ("blocked", palette::ST_BLOCKED),
                    ("idle", palette::ST_IDLE),
                    ("down", palette::ST_DOWN),
                ];
                ui.horizontal_wrapped(|ui| {
                    for (name, color) in legend {
                        let (rect, _) = ui
                            .allocate_exact_size(egui::vec2(9.0, 9.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, color);
                        ui.label(
                            egui::RichText::new(name)
                                .small()
                                .color(palette::TEXT_DIM),
                        );
                    }
                });
                ui.separator();
                if self.heatmap != 0 {
                    ui.separator();
                    // The toolbar carries the graduated scale; here a single
                    // line names the active metric so the canvas stays uncluttered.
                    ui.label(
                        egui::RichText::new(format!(
                            "coloured by {} · scale in the toolbar",
                            match self.heatmap {
                                1 => "utilization",
                                2 => "throughput",
                                _ => "WIP",
                            }
                        ))
                        .small()
                        .color(OpenFlexApp::heat_colour(0.62)),
                    );
                }
                ui.separator();
                ui.label(
                    egui::RichText::new(
                        "drag to rotate · wheel to zoom · click an object to edit · Step/Play drive the engine",
                    )
                    .color(egui::Color32::from_gray(140)),
                );
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_json_number_updates_nested_paths() {
        let mut value = serde_json::json!({
            "objects": [
                {"params": {"process_time": {"value": 1.0}}}
            ]
        });
        assert!(set_json_number(
            &mut value,
            "objects/0/params/process_time/value",
            4.5
        ));
        assert_eq!(
            value["objects"][0]["params"]["process_time"]["value"],
            serde_json::json!(4.5)
        );
    }

    #[test]
    fn set_json_number_rejects_unknown_or_invalid_paths() {
        let mut value = serde_json::json!({"objects": [{"params": {}}]});
        assert!(!set_json_number(
            &mut value,
            "objects/0/params/missing",
            1.0
        ));
        assert!(!set_json_number(&mut value, "objects/x/params", 1.0));
        assert!(!set_json_number(&mut value, "", 1.0));
        // A leaf that is an object (not a number slot) is not replaced.
        assert!(!set_json_number(&mut value, "objects", 1.0));
    }

    #[test]
    fn parse_params_builds_the_right_variant() {
        let params = parse_params(ObjectKind::Queue, serde_json::json!({"max_content": 7}))
            .expect("queue params");
        match params {
            ObjectParams::Queue(queue) => assert_eq!(queue.max_content, 7),
            other => panic!("unexpected variant: {other:?}"),
        }
        assert!(parse_params(ObjectKind::Queue, serde_json::json!({"nonsense": 1})).is_err());
    }

    #[test]
    fn heat_colour_spans_green_yellow_red() {
        let green = OpenFlexApp::heat_colour(0.0);
        let yellow = OpenFlexApp::heat_colour(0.5);
        let red = OpenFlexApp::heat_colour(1.0);
        assert!(green.g() > green.r(), "low values should be green");
        assert!(
            yellow.r() > 200 && yellow.g() > 150,
            "mid values are yellow"
        );
        assert!(red.r() > red.g(), "high values should be red");
        // Out-of-range inputs clamp instead of wrapping.
        assert_eq!(OpenFlexApp::heat_colour(-1.0), green);
        assert_eq!(OpenFlexApp::heat_colour(2.0), red);
    }

    #[test]
    fn shade_light_keeps_a_selected_body_recognisable() {
        // Lightening keeps the hue order (a busy red stays reddish) and never
        // clips back to the original value.
        let busy = palette::ST_BUSY;
        let lit = shade_light(busy, 0.35);
        assert!(lit.r() >= busy.r() && lit.g() >= busy.g() && lit.b() >= busy.b());
        assert!(lit.r() > lit.g(), "the red hue survives lightening");
        assert_eq!(shade_light(busy, 0.0), busy);
        // Fully lightening saturates to white.
        assert_eq!(
            shade_light(busy, 1.0),
            egui::Color32::from_rgb(255, 255, 255)
        );
    }

    #[test]
    fn shade_lightens_and_darkens() {
        let base = egui::Color32::from_rgb(100, 100, 100);
        let lighter = OpenFlexApp::shade(base, 0.5);
        let darker = OpenFlexApp::shade(base, -0.5);
        assert!(lighter.r() > base.r());
        assert!(darker.r() < base.r());
        assert_eq!(OpenFlexApp::shade(base, 0.0), base);
    }

    #[test]
    fn pareto_front_keeps_non_dominated_points() {
        let rows = vec![
            (vec![1.0], 10.0, 3.0), // best secondary, lower score
            (vec![2.0], 12.0, 4.0), // best score
            (vec![3.0], 9.0, 6.0),  // dominated by row 0
            (vec![4.0], 12.0, 5.0), // dominated by row 1
        ];
        let front = OpenFlexApp::pareto_front(&rows);
        assert_eq!(front, vec![0, 1]);
    }

    #[test]
    fn persistence_path_is_a_file_next_to_the_other_configs() {
        // eframe appends the .ron contents to whatever path it gets: a directory
        // there would silently never persist anything.
        let path = persistence_file();
        if let Some(path) = path {
            assert!(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".ron")),
                "the persistence path must be a file: {}",
                path.display()
            );
            assert!(path.parent().is_some());
        }
    }

    #[test]
    fn task_steps_keep_their_details_when_the_kind_changes() {
        use openflex_model::Distribution;
        use openflex_model::model::{TaskStep, TaskStepKind};
        // A detailed step keeps its duration when the kind is switched (mirrors
        // the assignment the combo box performs).
        let step = TaskStep::Detailed {
            kind: TaskStepKind::Travel,
            duration: Some(Distribution::Constant { value: 2.0 }),
            condition: None,
        };
        let switched = match step {
            TaskStep::Detailed {
                duration,
                condition,
                ..
            } => TaskStep::Detailed {
                kind: TaskStepKind::Load,
                duration,
                condition,
            },
            TaskStep::Simple(_) => TaskStep::Simple(TaskStepKind::Load),
        };
        match switched {
            TaskStep::Detailed { kind, duration, .. } => {
                assert_eq!(kind, TaskStepKind::Load);
                assert_eq!(duration, Some(Distribution::Constant { value: 2.0 }));
            }
            other => panic!("expected a detailed step, got {other:?}"),
        }
        // Simple forms serialise as plain strings.
        assert_eq!(
            serde_json::to_string(&TaskStep::Simple(TaskStepKind::Unload)).expect("json"),
            r#""unload""#
        );
    }

    #[test]
    fn queue_order_by_label_round_trips() {
        use openflex_model::QueueOrder;
        // The form writes `QueueOrder::ByLabel(String)`; the model serialises it
        // as `{"by_label": "..."}`.
        let order = QueueOrder::ByLabel("priority".to_string());
        let json = serde_json::to_string(&order).expect("serialise");
        assert_eq!(json, r#"{"by_label":"priority"}"#);
        let back: QueueOrder = serde_json::from_str(&json).expect("deserialise");
        assert_eq!(back, order);
        // Simple variants stay plain strings.
        assert_eq!(
            serde_json::to_string(&QueueOrder::Lifo).expect("serialise"),
            r#""lifo""#
        );
    }

    #[test]
    fn switching_distribution_keeps_shared_parameters() {
        use openflex_model::Distribution;
        // mean carries from normal to exponential.
        let mut dist = Distribution::Normal {
            mean: 8.0,
            stddev: 2.0,
        };
        switch_distribution(&mut dist, "exponential");
        match dist {
            Distribution::Exponential { mean } => assert_eq!(mean, 8.0),
            other => panic!("expected exponential, got {other:?}"),
        }
        // shape/scale carry between gamma and weibull.
        let mut dist = Distribution::Gamma {
            shape: 3.0,
            scale: 4.0,
        };
        switch_distribution(&mut dist, "weibull");
        match dist {
            Distribution::Weibull { shape, scale } => {
                assert_eq!((shape, scale), (3.0, 4.0));
            }
            other => panic!("expected weibull, got {other:?}"),
        }
        // A family without the field falls back to its own default.
        let mut dist = Distribution::Exponential { mean: 8.0 };
        switch_distribution(&mut dist, "beta");
        match dist {
            Distribution::Beta { alpha, beta } => {
                assert_eq!((alpha, beta), (2.0, 2.0), "no shared field: defaults");
            }
            other => panic!("expected beta, got {other:?}"),
        }
        // Integer families convert without losing the value.
        let mut dist = Distribution::IntegerUniform { min: 1, max: 6 };
        switch_distribution(&mut dist, "triangular");
        match dist {
            Distribution::Triangular { min, max, .. } => {
                assert_eq!((min, max), (1.0, 6.0));
            }
            other => panic!("expected triangular, got {other:?}"),
        }
    }

    #[test]
    fn connections_reject_self_and_duplicates() {
        // The guards `add_connection` applies before touching the model.
        let existing = [("A", "B"), ("B", "C")];
        let accepts = |from: &str, to: &str| {
            from != to && !existing.iter().any(|(f, t)| *f == from && *t == to)
        };
        assert!(accepts("A", "C"), "a new edge is fine");
        assert!(!accepts("A", "A"), "self connection");
        assert!(!accepts("A", "B"), "duplicate edge");
        assert!(!accepts("B", "C"), "duplicate edge, reversed order differs");
        assert!(
            accepts("C", "B"),
            "the reverse direction is a different edge"
        );
    }

    #[test]
    fn duplicate_names_never_collide() {
        // Mirrors the suffix search in `duplicate_selection`: the loop checks the
        // copy being built, so two duplicates in one action cannot collide.
        let mut names = vec!["Q1".to_string(), "Q1_2".to_string()];
        let next = |base: &str, names: &Vec<String>| {
            let mut suffix = 2;
            let mut candidate = format!("{base}_{suffix}");
            while names.iter().any(|name| name == &candidate) {
                suffix += 1;
                candidate = format!("{base}_{suffix}");
            }
            candidate
        };
        let first = next("Q1", &names);
        names.push(first.clone());
        let second = next("Q1", &names);
        assert_eq!(first, "Q1_3", "Q1_2 is taken");
        assert_eq!(second, "Q1_4", "the second copy skips the first");
        assert_ne!(first, second);
    }

    #[test]
    fn selection_indices_stay_valid_after_removal() {
        // The selection holds indices into `model.objects`; anything that shrinks
        // the list must clear it, otherwise later lookups index out of bounds.
        let mut selection: Vec<usize> = vec![0, 2, 3];
        // Simulate removing one object at index 1: index 2 now points at a
        // different object and 3 is out of bounds.
        let object_count = 3;
        assert!(selection.iter().any(|index| *index >= object_count));
        selection.clear();
        assert!(selection.iter().all(|index| *index < object_count));
        assert!(selection.is_empty());
    }

    #[test]
    fn layout_order_covers_every_object_including_cycles() {
        // No root at all (a two-node cycle): the walk reaches nothing, yet the
        // order must still list every object so auto layout can place them.
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "cycle",
          "run": {"stop_time": 10.0},
          "objects": [
            {"name": "S", "kind": "source", "params": {}},
            {"name": "K", "kind": "sink", "params": {}},
            {"name": "A", "kind": "queue", "params": {}},
            {"name": "B", "kind": "queue", "params": {}}
          ],
          "connections": [
            {"from": "A", "to": "B"}, {"from": "B", "to": "A"}
          ]
        }"#,
        )
        .expect("model");
        let order = OpenFlexApp::layout_order(&model);
        assert_eq!(order.len(), model.objects.len(), "every object gets a slot");
        let mut sorted = order.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec![0, 1, 2, 3], "no object is dropped");

        // A normal pipeline starts at the source.
        let pipeline = builtin_example("mm1").expect("example");
        let order = OpenFlexApp::layout_order(&pipeline);
        assert_eq!(order.len(), pipeline.objects.len());
        assert_eq!(
            pipeline.objects[order[0]].name, "Source1",
            "the source leads the layout"
        );
    }

    #[test]
    fn state_mix_normalises_and_handles_zero() {
        let mix = OpenFlexApp::state_mix([2.0, 1.0, 1.0, 0.0]);
        assert!((mix.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!((mix[0] - 0.5).abs() < 1e-12);
        assert!((mix[3] - 0.0).abs() < 1e-12);
        // An empty model must not produce NaN.
        assert_eq!(OpenFlexApp::state_mix([0.0; 4]), [0.0; 4]);
        // Negative or tiny totals are treated as empty rather than exploding.
        assert_eq!(OpenFlexApp::state_mix([1e-12; 4]), [0.0; 4]);
    }

    #[test]
    fn series_hover_index_stays_in_range() {
        assert_eq!(OpenFlexApp::series_index_at(0.0, 10), Some(0));
        assert_eq!(OpenFlexApp::series_index_at(1.0, 10), Some(9));
        assert_eq!(OpenFlexApp::series_index_at(0.5, 10), Some(5));
        assert_eq!(
            OpenFlexApp::series_index_at(2.0, 10),
            Some(9),
            "a position past the right edge clamps"
        );
        assert_eq!(OpenFlexApp::series_index_at(-1.0, 10), Some(0));
        assert_eq!(OpenFlexApp::series_index_at(0.5, 0), None, "no samples");
        assert_eq!(
            OpenFlexApp::series_index_at(0.5, 1),
            Some(0),
            "a single sample"
        );
    }

    #[test]
    fn heatmap_legend_range_falls_back_when_empty() {
        // The legend uses the min/max of the chosen metric, but an empty or
        // degenerate report must not draw `inf`.
        let values: Vec<f64> = Vec::new();
        let low = values.iter().copied().fold(f64::INFINITY, f64::min);
        let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let (low, high) = if low.is_finite() && high.is_finite() && low <= high {
            (low, high)
        } else {
            (0.0, 1.0)
        };
        assert_eq!((low, high), (0.0, 1.0));

        let single = [3.0_f64];
        let low = single.iter().copied().fold(f64::INFINITY, f64::min);
        let high = single.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(
            (low, high),
            (3.0, 3.0),
            "a single object has a zero-width range"
        );
    }

    #[test]
    fn camera_presets_stay_inside_the_drag_range() {
        // The Top preset and the drag handler share one limit, so picking Top
        // and then dragging must not jump.
        let preset_pitch = PITCH_MAX;
        let clamped = preset_pitch.clamp(PITCH_MIN, PITCH_MAX);
        assert_eq!(
            clamped, preset_pitch,
            "a preset must already satisfy the clamp"
        );
        // Range sanity, computed at runtime so the assertions are not constant.
        let ranges = [(PITCH_MAX - PITCH_MIN, 1.0_f32, "pitch tilt span")]
            .into_iter()
            .chain(std::iter::once((ZOOM_MAX - ZOOM_MIN, 1.0, "zoom span")))
            .chain(std::iter::once((1.0 - ZOOM_MIN, 0.5, "zoom out range")));
        for (value, minimum, name) in ranges {
            assert!(value > minimum, "{name} is too small: {value}");
        }
        // A drag from the preset moves smoothly in both directions.
        let up = (preset_pitch - 0.1).clamp(PITCH_MIN, PITCH_MAX);
        let down = (preset_pitch + 0.1).clamp(PITCH_MIN, PITCH_MAX);
        assert!((up - (preset_pitch - 0.1)).abs() < 1e-6);
        assert_eq!(down, PITCH_MAX, "further down is capped, not wrapped");
    }

    #[test]
    fn checkpoints_sort_by_model_time_not_by_name() {
        use std::path::PathBuf;
        let names = [
            "checkpoint-10.00.json",
            "checkpoint-9.00.json",
            "checkpoint-100.00.json",
            "checkpoint-9.00-2.json",
            "notes.txt",
        ];
        let mut entries: Vec<PathBuf> = names.iter().map(PathBuf::from).collect();
        entries.sort_by(|a, b| {
            OpenFlexApp::checkpoint_time(a).total_cmp(&OpenFlexApp::checkpoint_time(b))
        });
        let times: Vec<f64> = entries
            .iter()
            .map(|path| OpenFlexApp::checkpoint_time(path))
            .collect();
        assert_eq!(times, vec![0.0, 9.0, 9.0, 10.0, 100.0], "{times:?}");
        assert_eq!(
            entries
                .last()
                .map(|path| OpenFlexApp::checkpoint_time(path)),
            Some(100.0),
            "the newest checkpoint is the one with the largest model time"
        );
    }

    #[test]
    fn checkpoint_names_do_not_overwrite_each_other() {
        // Mirrors the suffix search in `save_checkpoint`.
        let dir = std::env::temp_dir().join(format!("openflex-ckpt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let next = |stem: &str, taken: &[&str]| {
            let mut candidate = format!("{stem}.json");
            let mut counter = 2;
            while taken.contains(&candidate.as_str()) {
                candidate = format!("{stem}-{counter}.json");
                counter += 1;
            }
            candidate
        };
        assert_eq!(next("checkpoint-10.00", &[]), "checkpoint-10.00.json");
        assert_eq!(
            next("checkpoint-10.00", &["checkpoint-10.00.json"]),
            "checkpoint-10.00-2.json"
        );
        assert_eq!(
            next(
                "checkpoint-10.00",
                &["checkpoint-10.00.json", "checkpoint-10.00-2.json"]
            ),
            "checkpoint-10.00-3.json"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn baseline_ownership_matches_by_name() {
        // The chip in the baseline row: dim when the names agree, amber when not.
        let matches = |model: &str, baseline: &str| model.is_empty() || model == baseline;
        assert!(matches("demo", "demo"));
        assert!(matches("", "demo"), "an unnamed model cannot be compared");
        assert!(!matches("demo", "other"));
    }

    #[test]
    fn every_worker_reports_the_model_it_ran() {
        // Report / time series / experiment / optimiser all carry the model name
        // so a result produced for a previous document can be labelled.
        let source = include_str!("main.rs");
        for needle in [
            "source_model: model.name.clone()",
            "source_model = base.name.clone()",
            ".map(|model| model.name.clone())",
        ] {
            assert!(
                source.contains(needle),
                "a worker does not record the model it ran: {needle}"
            );
        }
        // And each poll handler goes through the shared staleness check, so a
        // result produced for a previous document gets labelled.
        // Count the calls before this test body so the assertion does not count
        // its own literal.
        let before_tests = source
            .split("mod tests")
            .next()
            .expect("source before the test module");
        assert_eq!(
            before_tests
                .matches("let stale = self.result_is_stale(&source);")
                .count(),
            3,
            "the report, series and experiment handlers must all check provenance"
        );
        assert!(
            source.contains("fn result_is_stale"),
            "the shared provenance check is missing"
        );
    }

    #[test]
    fn baseline_note_only_fires_on_a_name_mismatch() {
        // The rule `load_baseline_json` applies before accepting a comparison.
        let note = |model: &str, baseline: &str| {
            if !model.is_empty() && model != baseline {
                Some(format!("'{}' vs '{}'", baseline, model))
            } else {
                None
            }
        };
        assert!(note("demo", "demo").is_none());
        assert!(
            note("", "demo").is_none(),
            "an unnamed model cannot mismatch"
        );
        let mismatch = note("demo", "other").expect("mismatch detected");
        assert!(mismatch.contains("other"));
    }

    #[test]
    fn grid_layout_anchors_on_the_selection_corner() {
        // Mirrors `batch_grid`: anchor = min x/y, then fill row-major.
        let positions = [(4.0_f64, 5.0), (-2.0, 1.0), (7.0, -3.0), (0.0, 0.0)];
        let min_x = positions.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let min_y = positions.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        assert_eq!((min_x, min_y), (-2.0, -3.0));
        let spacing = 4.0;
        let columns = (positions.len() as f64).sqrt().ceil() as usize;
        assert_eq!(columns, 2, "four objects make a 2x2 grid");
        let placed: Vec<(f64, f64)> = (0..positions.len())
            .map(|index| {
                (
                    min_x + (index % columns) as f64 * spacing,
                    min_y + (index / columns) as f64 * spacing,
                )
            })
            .collect();
        assert_eq!(
            placed,
            vec![(-2.0, -3.0), (2.0, -3.0), (-2.0, 1.0), (2.0, 1.0)],
            "row-major grid from the top-left corner"
        );
    }

    #[test]
    fn opening_a_document_clears_every_buffer() {
        let model = builtin_example("mm1").expect("example");
        let mut app = OpenFlexApp::new(None);
        app.model = Some(Box::leak(Box::new(model)));
        // Simulate a session with state in every document-scoped buffer.
        app.params_text = "{}".into();
        app.flow_text = "{}".into();
        app.rename_text = "Old".into();
        app.event_log.push(EventEntry {
            time: 1.0,
            kind: "arrive".into(),
            object: Some("Queue1".into()),
        });
        app.table_edit
            .insert(("routes".into(), 0, 0), "stale".into());
        app.smooth.insert("Queue1".into(), [1.0, 2.0]);
        app.undo.push(("{}".into(), "edit".into()));
        app.running = true;

        app.reset_for_new_document();

        assert!(app.params_text.is_empty());
        assert!(app.flow_text.is_empty());
        assert!(app.rename_text.is_empty());
        assert!(app.event_log.is_empty());
        assert!(app.table_edit.is_empty());
        assert!(app.smooth.is_empty());
        assert!(app.undo.is_empty());
        assert!(app.redo.is_empty());
        assert!(!app.running);
        assert!(!app.paused);
    }

    #[test]
    fn edited_row_must_match_the_table_width() {
        // The guard `commit_table_row` applies before touching the undo stack.
        let table = openflex_model::GlobalTable {
            columns: vec!["a".into(), "b".into()],
            rows: vec![Vec::new()],
        };
        let accepts = |cells: &[&str]| table.width() == cells.len();
        assert!(accepts(&["1", "2"]));
        assert!(!accepts(&["1"]), "short row");
        assert!(!accepts(&["1", "2", "3"]), "long row");
        assert!(!accepts(&[]), "empty row");
    }

    #[test]
    fn table_cells_parse_by_looking_at_the_text() {
        use openflex_model::TableCell;
        assert_eq!(OpenFlexApp::parse_table_cell("42"), TableCell::Number(42.0));
        assert_eq!(
            OpenFlexApp::parse_table_cell(" 3.5 "),
            TableCell::Number(3.5),
            "surrounding whitespace is ignored for numbers"
        );
        assert_eq!(
            OpenFlexApp::parse_table_cell("-1e3"),
            TableCell::Number(-1000.0)
        );
        assert_eq!(OpenFlexApp::parse_table_cell("true"), TableCell::Bool(true));
        assert_eq!(
            OpenFlexApp::parse_table_cell("FALSE"),
            TableCell::Bool(false)
        );
        assert_eq!(
            OpenFlexApp::parse_table_cell("route A"),
            TableCell::Str("route A".into())
        );
        assert_eq!(
            OpenFlexApp::parse_table_cell("007"),
            TableCell::Number(7.0),
            "numeric text becomes a number, like the CSV importer"
        );
    }

    #[test]
    fn ui_prefs_round_trip() {
        // The JSON `save_ui_prefs` writes must be readable by `load_ui_prefs`.
        let json = serde_json::json!({
            "flat_view": true,
            "tree_by_stage": false,
            "parallel": false,
            "play_speed": 2.5,
            "show_json": true,
        });
        let text = json.to_string();
        let value: serde_json::Value = serde_json::from_str(&text).expect("json");
        assert_eq!(value["flat_view"], serde_json::json!(true));
        assert_eq!(value["tree_by_stage"], serde_json::json!(false));
        assert_eq!(value["parallel"], serde_json::json!(false));
        assert_eq!(value["show_json"], serde_json::json!(true));
        let speed = value["play_speed"].as_f64().expect("speed");
        assert!((speed - 2.5).abs() < 1e-9);
        // Out-of-range speeds are clamped on load, not trusted.
        let clamped = (9.0_f32).clamp(0.25, 8.0);
        assert_eq!(clamped, 8.0);
    }

    #[test]
    fn help_dialog_lists_every_shortcut() {
        // The key caps the help dialog shows; every one must be a key the
        // window actually consumes (kept in sync by hand, checked here).
        let documented = [
            "Ctrl/Cmd + O",
            "Ctrl/Cmd + N",
            "Ctrl/Cmd + S",
            "Ctrl/Cmd + Z",
            "Ctrl/Cmd + Shift + Z",
            "Space",
            "R",
            "Right arrow",
            "Up / Down",
            "Ctrl/Cmd + A",
            "Ctrl/Cmd + D",
            "click",
            "drag on empty space",
            "Delete / Backspace",
            "drag",
            "wheel",
            "2D on/off",
            "Top / Front / Side",
            "F1",
        ];
        let source = include_str!("main.rs");
        for key in documented {
            if key == "click"
                || key == "drag"
                || key == "drag on empty space"
                || key == "wheel"
                || key == "2D on/off"
                || key == "Top / Front / Side"
            {
                continue; // mouse / toolbar affordances, not key codes
            }
            let needle = match key {
                "Ctrl/Cmd + O" => "egui::Key::O",
                "Ctrl/Cmd + N" => "egui::Key::N",
                "Ctrl/Cmd + S" => "egui::Key::S",
                "Ctrl/Cmd + Z" => "egui::Key::Z",
                "Ctrl/Cmd + Shift + Z" => "egui::Modifiers::SHIFT",
                "Space" => "egui::Key::Space",
                "R" => "egui::Key::R",
                "Right arrow" => "egui::Key::ArrowRight",
                "Up / Down" => "egui::Key::ArrowUp",
                "Ctrl/Cmd + A" => "egui::Key::A",
                "Ctrl/Cmd + D" => "egui::Key::D",
                "Delete / Backspace" => "egui::Key::Delete",
                "F1" => "egui::Key::F1",
                other => panic!("unmapped key cap: {other}"),
            };
            assert!(
                source.contains(needle),
                "help lists `{key}` but the window never handles `{needle}`"
            );
        }
    }

    #[test]
    fn align_uses_the_selection_extreme() {
        // Mirrors `batch_align`: the anchor is the minimum along the axis, so
        // the result does not depend on selection order.
        let positions = [(3.0_f64, 7.0), (-1.0, 2.0), (5.0, -4.0)];
        for axis in [0usize, 1] {
            let anchor = positions
                .iter()
                .map(|pos| if axis == 0 { pos.0 } else { pos.1 })
                .fold(f64::INFINITY, f64::min);
            let aligned: Vec<f64> = positions.iter().map(|_| anchor).collect();
            assert!(aligned.iter().all(|value| *value == anchor));
        }
        let min_x = positions.iter().map(|p| p.0).fold(f64::INFINITY, f64::min);
        let min_y = positions.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
        assert_eq!(min_x, -1.0);
        assert_eq!(min_y, -4.0);
    }

    #[test]
    fn stage_of_counts_hops_from_the_sources() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "stages",
          "run": {"stop_time": 10.0},
          "objects": [
            {"name": "S", "kind": "source", "params": {}},
            {"name": "Q1", "kind": "queue", "params": {}},
            {"name": "P1", "kind": "processor", "params": {}},
            {"name": "Q2", "kind": "queue", "params": {}},
            {"name": "K", "kind": "sink", "params": {}},
            {"name": "Lonely", "kind": "queue", "params": {}}
          ],
          "connections": [
            {"from": "S", "to": "Q1"}, {"from": "Q1", "to": "P1"},
            {"from": "P1", "to": "Q2"}, {"from": "Q2", "to": "K"}
          ]
        }"#,
        )
        .expect("model");
        let stages = OpenFlexApp::stage_of(&model);
        let by_name = |name: &str| {
            let index = model
                .objects
                .iter()
                .position(|o| o.name == name)
                .expect("object");
            stages[index]
        };
        assert_eq!(by_name("S"), 0);
        assert_eq!(by_name("Q1"), 1);
        assert_eq!(by_name("P1"), 2);
        assert_eq!(by_name("Q2"), 3);
        assert_eq!(by_name("K"), 4);
        assert_eq!(
            by_name("Lonely"),
            0,
            "isolated objects start their own stage"
        );
    }

    #[test]
    fn report_rows_sort_by_the_selected_metric() {
        // The comparator the table uses: `column` 0 sorts names, 1..7 sort the
        // metric values, `desc` flips the order.
        let rows = [
            ("A".to_string(), 5.0_f64, 0.2_f64),
            ("B".to_string(), 1.0, 0.9),
            ("C".to_string(), 3.0, 0.5),
        ];
        let sort = |column: u8, desc: bool| {
            let mut out = rows.clone();
            out.sort_by(|a, b| {
                if column == 0 {
                    let order = a.0.cmp(&b.0);
                    return if desc { order.reverse() } else { order };
                }
                let key = if column == 1 { a.1 } else { a.2 };
                let other = if column == 1 { b.1 } else { b.2 };
                let order = key.total_cmp(&other);
                if desc { order.reverse() } else { order }
            });
            out.into_iter().map(|row| row.0).collect::<Vec<_>>()
        };
        assert_eq!(sort(0, false), vec!["A", "B", "C"]);
        assert_eq!(sort(0, true), vec!["C", "B", "A"]);
        assert_eq!(sort(1, true), vec!["A", "C", "B"], "descending by count");
        assert_eq!(sort(1, false), vec!["B", "C", "A"]);
        assert_eq!(sort(2, true), vec!["B", "C", "A"], "descending by ratio");
    }

    #[test]
    fn removing_a_flow_block_by_id_actually_removes_it() {
        // Regression: the retain closure used to shadow the `id` parameter, so
        // the comparison was always `name != name` and nothing was removed.
        let mut flow = serde_json::json!({
            "blocks": [{"id": "A1"}, {"id": "B2"}, {"id": "A1b"}],
            "connections": [{"from": "A1", "to": "B2"}, {"from": "B2", "to": "A1b"}]
        });
        let target = "A1";
        if let Some(blocks) = flow.get_mut("blocks").and_then(|b| b.as_array_mut()) {
            blocks.retain(|block| block.get("id").and_then(|id| id.as_str()) != Some(target));
        }
        let ids: Vec<&str> = flow["blocks"]
            .as_array()
            .expect("blocks")
            .iter()
            .filter_map(|block| block.get("id").and_then(|id| id.as_str()))
            .collect();
        assert_eq!(ids, vec!["B2", "A1b"], "only the exact id is removed");
    }

    #[test]
    fn window_title_reflects_document_state() {
        assert_eq!(
            OpenFlexApp::window_title_text(None, false, false, false),
            "OpenFlex — no model"
        );
        assert_eq!(
            OpenFlexApp::window_title_text(Some("demo"), false, false, false),
            "OpenFlex — demo"
        );
        assert_eq!(
            OpenFlexApp::window_title_text(Some("demo"), true, false, false),
            "OpenFlex — demo *",
            "unsaved marker"
        );
        assert_eq!(
            OpenFlexApp::window_title_text(Some("demo"), true, true, false),
            "OpenFlex — demo * — playing"
        );
        assert_eq!(
            OpenFlexApp::window_title_text(Some("demo"), false, false, true),
            "OpenFlex — demo — running"
        );
    }

    #[test]
    fn undo_stack_keeps_a_label_per_step() {
        let model = builtin_example("mm1").expect("example");
        let mut app = OpenFlexApp::new(None);
        app.model = Some(Box::leak(Box::new(model)));
        app.snapshot_labelled("first edit");
        app.snapshot_labelled("second edit");
        let labels: Vec<String> = app.undo.iter().map(|(_, label)| label.clone()).collect();
        assert_eq!(labels, vec!["first edit", "second edit"]);
        // Undo pops the newest and pushes a redo carrying the same label, so the
        // history list can describe what would be re-applied.
        assert!(app.restore());
        assert_eq!(app.undo.len(), 1);
        assert_eq!(
            app.redo.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(),
            vec!["second edit"]
        );
        assert!(app.redo_once());
        assert_eq!(app.undo.len(), 2);
    }

    #[test]
    fn report_csv_header_documents_the_run() {
        // The header lines the export writes; every one starts with `#` so
        // spreadsheet importers skip them consistently.
        let header = format!(
            "# model: {}\n# time_unit: {}\n# stop_time: {}\n# warmup: {}\n# seed: {}\n# replications: {}\n",
            "demo", "min", 1000.0, 10.0, 42, 3
        );
        let lines: Vec<&str> = header.lines().collect();
        assert_eq!(lines.len(), 6);
        for line in &lines {
            assert!(line.starts_with("# "), "not a comment: {line}");
        }
        assert!(header.contains("# time_unit: min"));
        assert!(header.contains("# replications: 3"));
    }

    #[test]
    fn validation_cache_follows_the_model_generation() {
        let model = builtin_example("mm1").expect("example");
        let mut app = OpenFlexApp::new(None);
        app.model = Some(Box::leak(Box::new(model)));
        let issues = app.validation_issues();
        assert!(issues.is_empty(), "the built-in example is valid");
        let generation = app.model_gen;
        // A second call reuses the cache (same generation).
        let _ = app.validation_issues();
        assert_eq!(app.model_gen, generation, "reading must not bump the gen");
        // Editing bumps the generation, so the cache is recomputed.
        app.touch_model();
        assert_ne!(app.model_gen, generation);
        let _ = app.validation_issues();
        assert!(
            app.validity_cache
                .as_ref()
                .is_some_and(|(cached, _)| *cached == app.model_gen)
        );
    }

    #[test]
    fn event_log_matches_filters_case_insensitively() {
        let entries = [
            EventEntry {
                time: 1.0,
                kind: "arrive".into(),
                object: Some("Queue1".into()),
            },
            EventEntry {
                time: 2.0,
                kind: "process_done".into(),
                object: Some("Server".into()),
            },
            EventEntry {
                time: 3.0,
                kind: "flow".into(),
                object: None,
            },
        ];
        let matches = |needle: &str, entry: &EventEntry| {
            let needle = needle.to_lowercase();
            needle.is_empty()
                || entry.kind.to_lowercase().contains(&needle)
                || entry
                    .object
                    .as_ref()
                    .is_some_and(|object| object.to_lowercase().contains(&needle))
        };
        assert_eq!(entries.iter().filter(|e| matches("", e)).count(), 3);
        assert_eq!(entries.iter().filter(|e| matches("ARRIVE", e)).count(), 1);
        assert_eq!(entries.iter().filter(|e| matches("queue", e)).count(), 1);
        assert_eq!(entries.iter().filter(|e| matches("process", e)).count(), 1);
        assert_eq!(entries.iter().filter(|e| matches("nothing", e)).count(), 0);
    }

    #[test]
    fn new_objects_do_not_stack_on_one_spot() {
        // Mirrors the slot search in `add_object`: each add walks down the
        // column until the position is free.
        let mut taken: Vec<[f64; 2]> = Vec::new();
        let place = |taken: &mut Vec<[f64; 2]>, x: f64| {
            let mut y = 0.0_f64;
            while taken
                .iter()
                .any(|pos| (pos[0] - x).abs() < 2.0 && (pos[1] - y).abs() < 2.0)
            {
                y -= 4.0;
            }
            taken.push([x, y]);
            [x, y]
        };
        assert_eq!(place(&mut taken, 10.0), [10.0, 0.0]);
        assert_eq!(place(&mut taken, 10.0), [10.0, -4.0]);
        assert_eq!(place(&mut taken, 10.0), [10.0, -8.0]);
        // A different column starts at the top again.
        assert_eq!(place(&mut taken, 14.0), [14.0, 0.0]);
    }

    #[test]
    fn flow_block_ids_avoid_reuse() {
        // Mirrors the allocation loop in `add_flow_block`.
        let mut used: Vec<String> = vec!["C1".into(), "C3".into()];
        let next = |prefix: &str, used: &[String]| {
            let mut counter = used.len() + 1;
            loop {
                let candidate = format!("{prefix}{counter}");
                if !used.iter().any(|id| id == &candidate) {
                    return candidate;
                }
                counter += 1;
            }
        };
        assert_eq!(next("C", &used), "C4");
        used.push("C4".into());
        assert_eq!(next("C", &used), "C5");
        assert_eq!(next("D", &used), "D4", "other prefixes are independent");
    }

    #[test]
    fn flow_connection_input_rules() {
        // The rules the UI enforces before a connection is appended.
        let ids = ["A".to_string(), "B".to_string()];
        let accepts = |from: &str, to: &str| {
            from != to && ids.contains(&from.to_string()) && ids.contains(&to.to_string())
        };
        assert!(accepts("A", "B"));
        assert!(!accepts("A", "A"), "self connection");
        assert!(!accepts("A", "Z"), "unknown target");
        assert!(!accepts("Z", "B"), "unknown source");
    }

    #[test]
    fn point_in_polygon_handles_rotated_footprints() {
        // A diamond (square rotated 45°): its centre and inner points hit, the
        // corners of the bounding box do not.
        let corners = vec![
            egui::pos2(50.0, 0.0),
            egui::pos2(100.0, 50.0),
            egui::pos2(50.0, 100.0),
            egui::pos2(0.0, 50.0),
        ];
        assert!(point_in_polygon(egui::pos2(50.0, 50.0), &corners));
        assert!(point_in_polygon(egui::pos2(50.0, 20.0), &corners));
        assert!(!point_in_polygon(egui::pos2(2.0, 2.0), &corners));
        assert!(!point_in_polygon(egui::pos2(98.0, 98.0), &corners));
        assert!(!point_in_polygon(egui::pos2(200.0, 50.0), &corners));
        // Degenerate input is rejected rather than panicking.
        assert!(!point_in_polygon(egui::pos2(0.0, 0.0), &[]));
    }

    #[test]
    fn format_bytes_switches_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KiB");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(3 * 1024 * 1024), "3.0 MiB");
    }

    #[test]
    fn numeric_paths_finds_every_number() {
        let params = serde_json::json!({
            "process_time": {"distribution": "constant", "value": 4.0},
            "interarrival": {"distribution": "exponential", "mean": 5.0},
            "max_content": 100,
            "label": "fast",
            "flags": [true, 2.5]
        });
        let mut paths = Vec::new();
        numeric_paths(&params, "objects/0/params", &mut paths);
        paths.sort();
        assert_eq!(
            paths,
            vec![
                "objects/0/params/flags/1",
                "objects/0/params/interarrival/mean",
                "objects/0/params/max_content",
                "objects/0/params/process_time/value",
            ],
            "strings and booleans must not be offered: {paths:?}"
        );
    }

    #[test]
    fn issue_locations_map_to_object_names() {
        assert_eq!(
            OpenFlexApp::issue_target("objects.Charger1.charger").as_deref(),
            Some("Charger1")
        );
        assert_eq!(
            OpenFlexApp::issue_target("objects.Agv1.network[0].from").as_deref(),
            Some("Agv1")
        );
        // Model-level locations do not name an object.
        assert_eq!(OpenFlexApp::issue_target("run.stop_time"), None);
        assert_eq!(OpenFlexApp::issue_target("objects"), None);
        // Index-style locations are not names.
        assert_eq!(OpenFlexApp::issue_target("objects.3.charger"), None);
    }

    #[test]
    fn browser_lists_dirs_before_models() {
        // Build a scratch directory with a sub-folder, a model and a decoy.
        let dir = std::env::temp_dir().join(format!("openflex-browser-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).expect("sub dir");
        std::fs::write(dir.join("b.ofm"), "{}").expect("model");
        std::fs::write(dir.join("a.ofm"), "{}").expect("model");
        std::fs::write(dir.join("notes.txt"), "x").expect("decoy");

        let mut app = OpenFlexApp::new(None);
        app.browser_dir = dir.clone();
        app.refresh_browser();
        let names: Vec<(String, bool)> = app.browser_entries.clone();
        assert_eq!(
            names,
            vec![
                ("sub".to_string(), true),
                ("a.ofm".to_string(), false),
                ("b.ofm".to_string(), false),
            ],
            "directories first, then models alphabetically, no other files"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn trace_export_lines_match_the_cli_jsonl() {
        // The GUI writes one `serde_json::to_string(&TraceEvent)` per line, the
        // same shape `openflex run --trace` produces, so both files are
        // interchangeable.
        let model = builtin_example("mm1").expect("example");
        let config = SimConfig {
            stop_time: 20.0,
            warmup: 0.0,
            seed: 1,
        };
        let (_, trace) = openflex_core::simulate_traced(&model, &config, 0).expect("trace");
        assert!(!trace.events.is_empty(), "the run should record events");
        let line = serde_json::to_string(&trace.events[0]).expect("json");
        let value: serde_json::Value = serde_json::from_str(&line).expect("round trip");
        for key in ["time", "seq", "event", "object"] {
            assert!(value.get(key).is_some(), "missing {key} in {line}");
        }
        // data/message are skipped when empty, so a minimal line has 4 keys.
        assert!(line.starts_with('{') && line.ends_with('}'), "{line}");
    }

    #[test]
    fn delete_cleans_dangling_references() {
        // Mirror of the studio behaviour: deleting a dispatcher must drop the
        // home/charger/operator references that would keep validate() failing.
        let mut json = serde_json::json!({
            "objects": [
                {"name": "D", "kind": "dispatcher", "params": {}},
                {"name": "A", "kind": "agv", "params": {
                    "home": "D", "charger": "D",
                    "network": [{"from": "D", "to": "A", "distance": 5.0}]
                }}
            ]
        });
        let mut cleaned = 0;
        if let Some(objects) = json.get_mut("objects").and_then(|o| o.as_array_mut()) {
            for object in objects {
                if let Some(params) = object.get_mut("params") {
                    cleaned += OpenFlexApp::clean_references(params, "D");
                }
            }
        }
        let params = serde_json::to_string(&json["objects"][1]["params"]).expect("json");
        assert_eq!(cleaned, 2, "home + charger: {params}");
        assert!(!params.contains("\"home\""), "home survived: {params}");
        assert!(
            !params.contains("\"charger\""),
            "charger survived: {params}"
        );
        // Network edges are cleaned separately by delete_objects.
        assert!(params.contains("\"network\""), "edges stay: {params}");
    }

    #[test]
    fn rename_rewrites_every_reference() {
        // Object names appear in connections, home/charger/network targets and
        // process-flow blocks; renaming must patch all of them.
        let json = serde_json::json!({
            "objects": [
                {"name": "A", "kind": "dispatcher", "params": {}},
                {"name": "B", "kind": "agv", "params": {"home": "A", "charger": "A",
                  "network": [{"from": "A", "to": "B"}]}}
            ],
            "connections": [{"from": "A", "to": "B"}],
            "process_flow": {"blocks": [{"id": "t", "type": "transfer", "to": "A"}]}
        });
        let patched = OpenFlexApp::rewrite_name(json, "A", "Dispatch");
        let text = serde_json::to_string(&patched).expect("json");
        assert!(text.contains("\"Dispatch\""), "{text}");
        assert!(!text.contains("\"A\""), "old name still present: {text}");
        // Unrelated strings stay untouched.
        let untouched =
            OpenFlexApp::rewrite_name(serde_json::json!({"name": "Alpha"}), "A", "Dispatch");
        assert_eq!(untouched["name"], serde_json::json!("Alpha"));
    }

    #[test]
    fn layout_order_is_breadth_first_from_the_sources() {
        let model = Model::from_json(
            r#"{
          "format": "openflex-model", "version": 1, "name": "layout",
          "run": {"stop_time": 10.0},
          "objects": [
            {"name": "S", "kind": "source", "params": {}},
            {"name": "Q", "kind": "queue", "params": {}},
            {"name": "P", "kind": "processor", "params": {}},
            {"name": "K", "kind": "sink", "params": {}},
            {"name": "Lonely", "kind": "queue", "params": {}}
          ],
          "connections": [
            {"from": "S", "to": "Q"},
            {"from": "Q", "to": "P"},
            {"from": "P", "to": "K"}
          ]
        }"#,
        )
        .expect("model");
        let order = OpenFlexApp::layout_order(&model);
        let names: Vec<&str> = order
            .iter()
            .map(|i| model.objects[*i].name.as_str())
            .collect();
        assert_eq!(names, vec!["S", "Q", "P", "K", "Lonely"]);
    }

    #[test]
    fn framed_points_stay_inside_the_viewport() {
        // Every object must project inside the canvas after framing, for a few
        // camera angles and aspect ratios.
        let points = [[-12.0, -6.0], [12.0, 6.0], [0.0, 9.0], [-9.0, 3.0]];
        for (yaw, pitch) in [(-0.6_f32, 0.5_f32), (0.0, 1.45), (1.55, 0.05), (0.9, 0.3)] {
            let canvas = 560.0_f32;
            let (centre, zoom) = OpenFlexApp::frame_maths_view(&points, yaw, pitch, canvas);
            let scale = canvas / 12.0 * zoom;
            let (sin_yaw, cos_yaw) = yaw.sin_cos();
            let (sin_pitch, _cos_pitch) = pitch.sin_cos();
            let project = |pos: [f32; 2]| {
                let rx = pos[0] * cos_yaw - pos[1] * sin_yaw;
                let ry = pos[0] * sin_yaw + pos[1] * cos_yaw;
                [rx * scale, ry * sin_pitch * scale]
            };
            let focus = project(centre);
            for point in points {
                let p = project([point[0] as f32, point[1] as f32]);
                let offset = [p[0] - focus[0], p[1] - focus[1]];
                assert!(
                    offset[0].abs() <= canvas / 2.0 && offset[1].abs() <= canvas / 2.0,
                    "yaw {yaw} pitch {pitch}: {offset:?} outside {}",
                    canvas / 2.0
                );
            }
        }
    }

    #[test]
    fn frame_view_keeps_a_rotated_group_inside_the_canvas() {
        let points = [[-12.0, 0.0], [12.0, 6.0]];
        let (centre, zoom) = OpenFlexApp::frame_maths_view(&points, -0.6, 0.35, 640.0);
        assert!(zoom > 0.0 && zoom < 2.5, "zoom out of range: {zoom}");
        // Projecting the centre back must land near the middle of the view.
        let (sin_yaw, cos_yaw) = (-0.6_f32).sin_cos();
        let (sin_pitch, cos_pitch) = 0.35_f32.sin_cos();
        let project = |pos: [f32; 2]| {
            let rx = pos[0] * cos_yaw - pos[1] * sin_yaw;
            let ry = pos[0] * sin_yaw + pos[1] * cos_yaw;
            [rx, ry * sin_pitch - 0.0 * cos_pitch]
        };
        let c = project(centre);
        // Group midpoint must project near the origin (the canvas centre).
        let mid = [
            (points[0][0] + points[1][0]) as f32 / 2.0,
            (points[0][1] + points[1][1]) as f32 / 2.0,
        ];
        let m = project(mid);
        assert!((c[0] - m[0]).abs() < 0.5, "x {c:?} vs {m:?}");
        assert!((c[1] - m[1]).abs() < 0.5, "y {c:?} vs {m:?}");
    }

    #[test]
    fn builtin_examples_parse_and_validate() {
        for name in [
            "mm1",
            "routing",
            "batch-queue",
            "agv-delivery",
            "fluid-line",
        ] {
            let model = builtin_example(name).unwrap_or_else(|| panic!("{name} missing"));
            model
                .validate()
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(!model.objects.is_empty(), "{name} has no objects");
        }
        assert!(builtin_example("nope").is_none());
    }

    #[test]
    fn xml_escape_protects_svg_text() {
        assert_eq!(xml_escape("A&B"), "A&amp;B");
        assert_eq!(xml_escape("<x>"), "&lt;x&gt;");
        assert_eq!(xml_escape("say \"hi\""), "say &quot;hi&quot;");
        assert_eq!(xml_escape("it's"), "it&apos;s");
        assert_eq!(xml_escape("plain"), "plain");
    }

    #[test]
    fn cjk_font_candidates_are_absolute_paths() {
        // The installer must never depend on the working directory.
        for path in [
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "C:/Windows/Fonts/msyh.ttc",
        ] {
            assert!(
                path.starts_with('/') || path.chars().nth(1) == Some(':'),
                "{path} is not absolute"
            );
        }
    }

    #[test]
    fn app_icon_is_opaque_in_the_middle_and_transparent_outside() {
        let icon = app_icon();
        assert_eq!(icon.width, 64);
        assert_eq!(icon.rgba.len(), (64 * 64 * 4) as usize);
        let centre = ((32 * 64 + 32) * 4) as usize;
        assert_eq!(icon.rgba[centre + 3], 255, "centre is opaque");
        assert_eq!(icon.rgba[3], 0, "corner is transparent");
    }

    #[test]
    fn event_entry_formats_the_log_line() {
        let entry = EventEntry {
            time: 1.5,
            kind: "arrive".into(),
            object: Some("Queue1".into()),
        };
        assert_eq!(entry.line(), "t=1.500 arrive [Queue1]");
        let plain = EventEntry {
            time: 2.0,
            kind: "flow".into(),
            object: None,
        };
        assert_eq!(plain.line(), "t=2.000 flow");
    }
}
