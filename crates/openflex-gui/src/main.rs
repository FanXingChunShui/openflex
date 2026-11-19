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
    ObjectKind, ObjectParams, QueueOrder, RackPickPolicy, RackType, ValveOpeningCurve,
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
        _ => return None,
    };
    Model::from_json(json).ok()
}

fn main() -> eframe::Result<()> {
    let model = std::env::args().nth(1);
    let options = eframe::NativeOptions {
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
    ui.add_space(2.0);
    ui.label(
        egui::RichText::new(text.to_uppercase())
            .size(11.0)
            .strong()
            .color(palette::TEXT_DIM),
    );
    ui.add_space(1.0);
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
        ui.label(label);
        egui::ComboBox::from_id_salt(format!("dist-{label}"))
            .selected_text(current)
            .show_ui(ui, |ui| {
                for name in DIST_CHOICES {
                    if ui.selectable_label(current == *name, *name).clicked() && current != *name {
                        *dist = make_distribution(name);
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

fn optional_u32(ui: &mut egui::Ui, label: &str, value: &mut Option<u32>) -> bool {
    let mut enabled = value.is_some();
    let mut changed = ui.checkbox(&mut enabled, label).changed();
    if changed {
        *value = if enabled { Some(1) } else { None };
    }
    if let Some(inner) = value {
        changed |= ui.add(egui::DragValue::new(inner).speed(1.0)).changed();
    }
    changed
}

fn optional_f64(ui: &mut egui::Ui, label: &str, value: &mut Option<f64>) -> bool {
    let mut enabled = value.is_some();
    let mut changed = ui.checkbox(&mut enabled, label).changed();
    if changed {
        *value = if enabled { Some(1.0) } else { None };
    }
    if let Some(inner) = value {
        changed |= ui.add(egui::DragValue::new(inner).speed(0.1)).changed();
    }
    changed
}

fn optional_string(ui: &mut egui::Ui, label: &str, value: &mut Option<String>) -> bool {
    let mut text = value.clone().unwrap_or_default();
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(label);
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
                                if matches!(p.order, QueueOrder::ByLabel(_)) {
                                    ui.label("by_label(…) — edit via JSON");
                                }
                            });
                    });
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
                ui.label(format!(
                    "task steps: {} (edit via JSON)",
                    p.task_sequence.len()
                ));
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
    new_from: String,
    new_to: String,
    new_port: String,
    new_conn_kind: u8,
    handle: Option<SimulationHandle<'static>>,
    sim_state: Option<SimulationState>,
    event_log: Vec<EventEntry>,
    heatmap: u8,
    model_name: String,
    model_stop: f32,
    model_warmup: f32,
    model_seed: String,
    model_replications: u32,
    model_unit: u8,
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
    report_rx: Option<Receiver<Result<SimReport, String>>>,
    report_running: bool,
    exp_start: f32,
    exp_step: f32,
    exp_count: u32,
    exp_rows: Vec<(f64, f64, f64)>, // (value, completed mean, ci95)
    exp_rx: Option<Receiver<Vec<(f64, f64, f64)>>>,
    exp_running: bool,
    exp_mode_param: bool,
    exp_path: String,
    exp_reps: u32,
    smooth: std::collections::HashMap<String, [f32; 2]>,
    last_frame: f64,
    undo: Vec<String>,
    redo: Vec<String>,
    recent: Vec<PathBuf>,
    save_as: String,
    show_save_as: bool,
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
    series_rx: Option<Receiver<Vec<SeriesRow>>>,
    series_object: String,
    series_metric: u8,
    opt_steps: u32,
    opt_mode: u8,
    opt_samples: u32,
    opt_vars: Vec<OptVar>,
    opt_secondary_stay: bool,
    opt_rows: Vec<OptRow>,
    opt_best: Option<OptRow>,
    opt_rx: Option<Receiver<Vec<OptRow>>>,
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
            new_from: String::new(),
            new_to: String::new(),
            new_port: String::new(),
            new_conn_kind: 0,
            handle: None,
            sim_state: None,
            event_log: Vec::new(),
            heatmap: 0,
            model_name: String::new(),
            model_stop: 300.0,
            model_warmup: 0.0,
            model_seed: "1".into(),
            model_replications: 1,
            model_unit: 0,
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
        if let Some(path) = model {
            app.load(PathBuf::from(path));
        }
        app
    }

    /// Records the current model in the undo stack (called before an edit).
    fn snapshot(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        if let Ok(json) = model.to_json() {
            self.undo.push(json);
            if self.undo.len() > 50 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
    }

    fn restore(&mut self) -> bool {
        let Some(json) = self.undo.pop() else {
            return false;
        };
        match Model::from_json(&json) {
            Ok(model) => {
                if let Some(current) = self.model
                    && let Ok(text) = current.to_json()
                {
                    self.redo.push(text);
                }
                self.model = Some(Box::leak(Box::new(model)));
                self.selected = None;
                self.form = None;
                self.handle = None;
                self.sim_state = None;
                self.status = "Undo".into();
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    fn redo_once(&mut self) -> bool {
        let Some(json) = self.redo.pop() else {
            return false;
        };
        match Model::from_json(&json) {
            Ok(model) => {
                if let Some(current) = self.model
                    && let Ok(text) = current.to_json()
                {
                    self.undo.push(text);
                }
                self.model = Some(Box::leak(Box::new(model)));
                self.selected = None;
                self.form = None;
                self.handle = None;
                self.sim_state = None;
                self.status = "Redo".into();
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
                self.selected = None;
                self.form = None;
                self.report = None;
                self.handle = None;
                self.sim_state = None;
                self.undo.clear();
                self.redo.clear();
                self.model_name = "untitled".into();
                self.model_stop = 300.0;
                self.model_warmup = 0.0;
                self.model_seed = "1".into();
                self.model_replications = 1;
                self.model_unit = 0;
                self.status = "New model".into();
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
            self.handle = None;
            self.sim_state = None;
            self.report = None;
            self.selection.clear();
            self.selected = None;
            self.form = None;
            self.status = format!("Loaded built-in example: {name}");
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
                self.selected = None;
                self.form = None;
                self.params_text.clear();
                self.handle = None;
                self.sim_state = None;
                self.running = false;
                self.paused = false;
                self.report = None;
            }
            Err(error) => self.error = Some(format!("invalid model: {error}")),
        }
    }

    fn save(&mut self) {
        let (Some(path), Some(model)) = (self.path.clone(), self.model) else {
            return;
        };
        match model.to_json() {
            Ok(text) => match std::fs::write(&path, text) {
                Ok(()) => self.status = format!("Saved {}", path.display()),
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
        let count = blocks.len() + 1;
        let id = format!(
            "{}{count}",
            kind.chars().next().unwrap_or('B').to_uppercase()
        );
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
            blocks.retain(|block| block.get("id").and_then(|id| id.as_str()) != Some(id));
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
        if let Some(connections) = flow.get_mut("connections").and_then(|c| c.as_array_mut()) {
            connections.push(serde_json::json!({"from": from, "to": to}));
        }
        self.flow_text = serde_json::to_string_pretty(&flow).unwrap_or_default();
        self.status = "Flow connection added".into();
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

    fn flow_default_pos(index: usize) -> [f32; 2] {
        let column = index % 3;
        let row = index / 3;
        [20.0 + column as f32 * 150.0, 20.0 + row as f32 * 70.0]
    }

    fn start_timeseries(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let interval = (self.series_interval as f64).max(0.1);
        let stop = model.run.stop_time;
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
            if let Ok(mut handle) = SimulationHandle::new(model, &config, 0) {
                let mut now = 0.0;
                while now < stop - 1e-9 {
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
            let _ = tx.send(rows);
        });
    }

    fn poll_timeseries(&mut self) {
        let Some(rx) = self.series_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(rows) => {
                self.status = format!("Time series: {} samples", rows.len());
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
        let (tx, rx) = channel();
        self.opt_rx = Some(rx);
        self.opt_running = true;
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
                                let _ = tx.send(rows);
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
            let _ = tx.send(rows);
        });
    }

    fn poll_optimize(&mut self) {
        let Some(rx) = self.opt_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(mut rows) => {
                rows.sort_by(|a, b| b.1.total_cmp(&a.1));
                rows.truncate(20);
                self.opt_best = rows.first().cloned();
                self.status = match self.opt_best.as_ref() {
                    Some((values, score, _)) => format!(
                        "Best: {} -> completed {score:.1}",
                        values
                            .iter()
                            .map(|value| format!("{value:.3}"))
                            .collect::<Vec<_>>()
                            .join(" / ")
                    ),
                    None => "Optimisation finished without results".into(),
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
        let mut text = String::from("time,object,content,throughput,utilization\n");
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
            Ok(()) => self.status = format!("Time series written to {}", path.display()),
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
        let mut text = String::from("object,metric,baseline,current,delta,delta_percent\n");
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
            Ok(()) => self.status = format!("Comparison written to {}", path.display()),
            Err(error) => self.error = Some(format!("could not write comparison: {error}")),
        }
    }

    fn export_report_csv(&mut self) {
        let Some(report) = self.report.as_ref() else {
            return;
        };
        // Every metric carries its 95% half-width next to the mean so the
        // export matches the CLI/Studio tables (which show mean ± CI95).
        let mut text = String::from(
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
            Ok(()) => self.status = format!("Report CSV written to {}", path.display()),
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
                    Ok(()) => self.status = format!("Baseline written to {}", path.display()),
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
                self.baseline = Some(report);
                self.show_compare = true;
                self.status = format!("Baseline loaded from {}", path.display());
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
                    Ok(()) => self.status = format!("Report JSON written to {}", path.display()),
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
        self.snapshot();
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
                self.checkpoint_status = format!("restored {file}");
                self.status = self.checkpoint_status.clone();
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
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
        entries.sort();
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
                self.status = format!("Model settings applied (stop {})", copy.run.stop_time);
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
        let max_x = model
            .objects
            .iter()
            .map(|object| object.pos[0])
            .fold(-8.0_f64, f64::max);
        self.snapshot();
        let mut copy = model.clone();
        copy.objects.push(openflex_model::model::Object {
            name: name.clone(),
            kind,
            pos: [max_x + 4.0, 0.0],
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
        let zoom = (usable * 12.0 / (span * canvas.max(1.0))).clamp(0.15, 2.5);
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
        self.snapshot();
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
                self.model = Some(Box::leak(Box::new(copy)));
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
            self.snapshot();
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
        // Isolated objects (no connections at all) come last.
        for (index, used) in seen.iter().enumerate() {
            if !used {
                order.push(index);
            }
        }
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
        let anchor = model.objects[indices[0]].pos;
        self.snapshot();
        let mut copy = model.clone();
        for index in indices {
            let object = &mut copy.objects[index];
            if along_x {
                object.pos[0] = anchor[0];
            } else {
                object.pos[1] = anchor[1];
            }
        }
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.status = if along_x {
                    "Aligned on X".into()
                } else {
                    "Aligned on Y".into()
                };
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
        self.snapshot();
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
        let origin = model.objects[self.selection[0]].pos;
        self.snapshot();
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
        self.snapshot();
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
        self.snapshot();
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
        self.snapshot();
        let mut copy = model.clone();
        copy.objects.retain(|object| !names.contains(&object.name));
        copy.connections.retain(|connection| {
            !names.contains(&connection.from) && !names.contains(&connection.to)
        });
        match copy.validate() {
            Ok(()) => {
                self.model = Some(Box::leak(Box::new(copy)));
                self.selection.clear();
                self.selected = None;
                self.form = None;
                self.status = format!("Deleted {} object(s)", names.len());
                self.error = None;
            }
            Err(error) => self.error = Some(error.to_string()),
        }
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
        let (tx, rx) = channel();
        self.exp_rx = Some(rx);
        self.exp_running = true;
        self.status = format!("Experiment: {count} runs…");
        std::thread::spawn(move || {
            let mut rows = Vec::new();
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
                if let Ok(report) = outcome {
                    rows.push((value, report.completed.mean, report.completed.ci95));
                }
            }
            let _ = tx.send(rows);
        });
    }

    fn poll_experiment(&mut self) {
        let Some(rx) = self.exp_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(rows) => {
                self.status = format!("Experiment finished: {} runs", rows.len());
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

    fn start_report(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let (tx, rx) = channel();
        self.report_rx = Some(rx);
        self.report_running = true;
        self.status = "Running a full replication…".into();
        let parallel = self.parallel_runs;
        std::thread::spawn(move || {
            // Replications are independent, so running them on all cores is
            // bit-identical to the sequential loop (task-59).
            let result = if parallel {
                openflex_core::run_model_parallel(model, 0)
            } else {
                openflex_core::run_model(model)
            }
            .map_err(|error| error.to_string());
            let _ = tx.send(result);
        });
    }

    fn poll_report(&mut self) {
        let Some(rx) = self.report_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(report)) => {
                self.status = format!(
                    "Run finished: {:.1} completed, {} objects",
                    report.completed.mean,
                    report.objects.len()
                );
                self.report = Some(report);
                self.report_running = false;
                self.report_rx = None;
            }
            Ok(Err(error)) => {
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
        let entry = EventEntry {
            time: preview.time,
            kind: preview.kind.clone(),
            object: preview.object.clone(),
        };
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
        if let Err(error) = handle.step_until(target) {
            self.error = Some(error.to_string());
            return;
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

    fn apply_form(&mut self) {
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
        self.snapshot();
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
        self.snapshot();
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
        self.snapshot();
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
        self.snapshot();
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
<rect width="{width}" height="{height}" fill="#12161c"/>
"##
        ));
        for connection in &model.connections {
            let (Some(from), Some(to)) =
                (model.object(&connection.from), model.object(&connection.to))
            else {
                continue;
            };
            let a = self.project(from.pos, 0.0, center, scale);
            let b = self.project(to.pos, 0.0, center, scale);
            svg.push_str(&format!(
                r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="#5a7896" stroke-width="1.2"/>
"##,
                a.x, a.y, b.x, b.y
            ));
        }
        for object in &model.objects {
            let point = self.project(object.pos, object.z, center, scale);
            let base = match self.heatmap_fill(&object.name) {
                Some(colour) => colour,
                None => Self::kind_color(object.kind),
            };
            let rot = object.rot.unwrap_or(0.0);
            let (r, g, b) = (base.r(), base.g(), base.b());
            let (x, y) = (point.x, point.y);
            let w = 38.0 * self.zoom;
            let h = 18.0 * self.zoom;
            let name = &object.name;
            svg.push_str(&format!(
                "<g transform=\"rotate({rot:.1} {x:.1} {y:.1})\">\
<rect x=\"{rx:.1}\" y=\"{ry:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" rx=\"4\" fill=\"#{r:02x}{g:02x}{b:02x}\" stroke=\"#14181e\"/>\
</g>\
<text x=\"{x:.1}\" y=\"{ty:.1}\" fill=\"#ffffff\" font-size=\"11\" text-anchor=\"middle\" font-family=\"sans-serif\">{name}</text>\n",
                rx = x - w / 2.0,
                ry = y - h / 2.0,
                ty = y - 12.0 * self.zoom,
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
    /// hit-testing and labels.
    #[allow(clippy::too_many_arguments)]
    fn draw_object_shape(
        painter: &egui::Painter,
        kind: ObjectKind,
        base: egui::Pos2,
        zoom: f32,
        fill: egui::Color32,
        selected: bool,
        rotation: f32,
        depth: f32,
    ) -> egui::Rect {
        let outline = egui::Stroke::new(
            1.0_f32,
            if selected {
                egui::Color32::from_rgb(255, 235, 190)
            } else {
                egui::Color32::from_rgb(16, 20, 26)
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
                painter.circle_filled(base, radius, fill);
                painter.circle_stroke(base, radius, outline);
                painter.circle_filled(
                    base + egui::vec2(-radius * 0.3, -radius * 0.3),
                    radius * 0.42,
                    OpenFlexApp::shade(fill, 0.3),
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
                painter.rect_filled(body, 6.0, fill);
                painter.rect_stroke(body, 6.0, outline);
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
        match kind {
            ObjectKind::Source => egui::Color32::from_rgb(70, 150, 100),
            ObjectKind::Sink => egui::Color32::from_rgb(140, 80, 140),
            ObjectKind::Processor | ObjectKind::MultiProcessor => {
                egui::Color32::from_rgb(70, 120, 200)
            }
            ObjectKind::Queue | ObjectKind::Rack => egui::Color32::from_rgb(90, 160, 190),
            ObjectKind::Conveyor | ObjectKind::Escalator => egui::Color32::from_rgb(180, 150, 90),
            ObjectKind::Transporter
            | ObjectKind::Person
            | ObjectKind::Agv
            | ObjectKind::Robot
            | ObjectKind::Crane
            | ObjectKind::AsrsVehicle => egui::Color32::from_rgb(200, 130, 90),
            ObjectKind::FluidTank
            | ObjectKind::FluidPipe
            | ObjectKind::FluidValve
            | ObjectKind::FluidGenerator
            | ObjectKind::FluidSink => egui::Color32::from_rgb(90, 180, 170),
            _ => egui::Color32::from_rgb(120, 130, 140),
        }
    }
}

impl eframe::App for OpenFlexApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.last_frame = ctx.input(|input| input.stable_dt) as f64;
        self.poll_report();
        self.poll_experiment();
        self.poll_timeseries();
        self.poll_optimize();
        if self.report_running || self.exp_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
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
                    ui.separator();
                    tool_group(ui, "FILE");
                    ui.add(egui::TextEdit::singleline(&mut self.path_input).desired_width(280.0));
                    if ui.button("Open").clicked() && !self.path_input.is_empty() {
                        self.load(PathBuf::from(self.path_input.clone()));
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
                        ui.add(
                            egui::ProgressBar::new(progress as f32)
                                .desired_width(140.0)
                                .text(format!("{:.1}/{:.1}", state.now, model.run.stop_time)),
                        );
                    }
                    ui.separator();
                    tool_group(ui, "STATE");
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
                    if ui.button("Top").clicked() {
                        self.yaw = 0.0;
                        self.pitch = 1.45;
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
                                if ui.selectable_label(self.heatmap == value, name).clicked() {
                                    self.heatmap = value;
                                }
                            }
                        });
                    if let Some(state) = self.sim_state.as_ref() {
                        ui.separator();
                        ui.label(format!("t = {:.2}", state.now));
                        ui.label(format!("created {}", state.items_created));
                        ui.label(format!("completed {}", state.items_completed));
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
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(state) = self.sim_state.as_ref() {
                            ui.label(
                                egui::RichText::new(format!("t = {:.2}", state.now))
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
            egui::TopBottomPanel::top("error").show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(200, 60, 60), error);
                    if ui.button("Dismiss").clicked() {
                        self.error = None;
                    }
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
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("filter")
                            .desired_width(110.0),
                    );
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
                        ui.horizontal(|ui| {
                            ui.label("name");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.model_name)
                                    .desired_width(150.0),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("unit");
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
                            ui.label("stop");
                            ui.add(
                                egui::DragValue::new(&mut self.model_stop)
                                    .speed(10.0)
                                    .range(1.0..=1.0e9),
                            );
                            ui.label("warmup");
                            ui.add(
                                egui::DragValue::new(&mut self.model_warmup)
                                    .speed(1.0)
                                    .range(0.0..=1.0e9),
                            );
                        });
                        ui.horizontal(|ui| {
                            ui.label("seed");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.model_seed)
                                    .desired_width(110.0),
                            );
                            ui.label("reps");
                            ui.add(
                                egui::DragValue::new(&mut self.model_replications)
                                    .speed(1.0)
                                    .range(1..=1000),
                            );
                        });
                        if ui.button("Apply settings").clicked() {
                            self.apply_model_settings();
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("new-kind")
                        .selected_text(self.new_kind.clone())
                        .show_ui(ui, |ui| {
                            for kind in KINDS {
                                if ui.selectable_label(self.new_kind == *kind, *kind).clicked() {
                                    self.new_kind = (*kind).to_string();
                                }
                            }
                        });
                    ui.add(
                        egui::TextEdit::singleline(&mut self.new_name)
                            .hint_text("name (optional)")
                            .desired_width(110.0),
                    );
                    if ui.button("Add").clicked() {
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
                    for example in ["mm1", "routing", "batch-queue"] {
                        if ui.button(format!("Open example: {example}")).clicked() {
                            self.load_example(example);
                        }
                    }
                    return;
                };
                let needle = self.search.to_lowercase();
                egui::ScrollArea::vertical()
                    .id_salt("tree")
                    .max_height(ui.available_height() * 0.45)
                    .show(ui, |ui| {
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
                                for index in indices.clone() {
                                    let object = &model.objects[index];
                                    let label = format!(
                                        "{}  ({:.0}, {:.0}){}",
                                        object.name,
                                        object.pos[0],
                                        object.pos[1],
                                        object
                                            .rot
                                            .map(|rot| format!("  {rot:.0}°"))
                                            .unwrap_or_default()
                                    );
                                    if ui
                                        .selectable_label(
                                            self.selected == Some(index),
                                            label,
                                        )
                                        .clicked()
                                    {
                                        self.select(index);
                                    }
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
                if let (Some(model), Some(index), Some(form)) =
                    (self.model, self.selected, self.form.as_mut())
                {
                    let name = model.objects[index].name.clone();
                    ui.horizontal(|ui| {
                        ui.heading(name);
                        ui.label(format!("({})", form.kind.as_str()));
                    });
                    ui.horizontal(|ui| {
                        ui.label("x");
                        ui.add(egui::DragValue::new(&mut form.pos[0]).speed(1.0));
                        ui.label("y");
                        ui.add(egui::DragValue::new(&mut form.pos[1]).speed(1.0));
                        ui.label("z");
                        ui.add(egui::DragValue::new(&mut form.z).speed(0.1));
                    });
                    ui.separator();
                    egui::ScrollArea::vertical()
                        .id_salt("form")
                        .max_height(ui.available_height() * 0.55)
                        .show(ui, |ui| {
                            form.show(ui);
                        });
                    ui.horizontal(|ui| {
                        if ui.button("Apply").clicked() {
                            self.apply_form();
                        }
                        ui.checkbox(&mut self.show_json, "JSON");
                    });
                    // Live validation: the model is checked against the same
                    // rules the CLI uses, so problems show up before Apply.
                    if let Some(model) = self.model {
                        let issues = openflex_model::validate_collect(model);
                        if !issues.is_empty() {
                            ui.separator();
                            ui.label(
                                egui::RichText::new(format!(
                                    "{} validation issue(s)",
                                    issues.len()
                                ))
                                .color(egui::Color32::from_rgb(220, 120, 90)),
                            );
                            egui::ScrollArea::vertical()
                                .id_salt("issues")
                                .max_height(90.0)
                                .show(ui, |ui| {
                                    for issue in issues.iter().take(20) {
                                        ui.label(
                                            egui::RichText::new(format!(
                                                "{}: {}",
                                                issue.location, issue.message
                                            ))
                                            .small(),
                                        );
                                    }
                                });
                        }
                    }
                    if self.show_json {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.params_text)
                                .code_editor()
                                .desired_rows(8),
                        );
                        if ui.button("Apply JSON").clicked() {
                            self.apply_json();
                        }
                    }
                }
                if !self.selection.is_empty() {
                    ui.separator();
                    ui.label(
                            egui::RichText::new(format!(
                                "{} objects selected",
                                self.selection.len()
                            ))
                            .strong()
                            .color(palette::AMBER),
                        );
                    ui.horizontal(|ui| {
                        if ui.small_button("Align X").clicked() {
                            self.batch_align(true);
                        }
                        if ui.small_button("Align Y").clicked() {
                            self.batch_align(false);
                        }
                        if ui.small_button("Distribute X").clicked() {
                            self.batch_distribute(true);
                        }
                        if ui.small_button("Distribute Y").clicked() {
                            self.batch_distribute(false);
                        }
                        if ui.small_button("Grid 4").clicked() {
                            self.batch_grid(4.0);
                        }
                        if ui.small_button("Duplicate").clicked() {
                            self.duplicate_selection();
                        }
                        if ui.small_button("Delete").clicked() {
                            self.batch_delete();
                        }
                        if ui.small_button("Clear").clicked() {
                            self.selection.clear();
                            self.status = "Selection cleared".into();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("move by");
                        ui.add(
                            egui::DragValue::new(&mut self.batch_dx)
                                .speed(0.5)
                                .prefix("dx "),
                        );
                        ui.add(
                            egui::DragValue::new(&mut self.batch_dy)
                                .speed(0.5)
                                .prefix("dy "),
                        );
                        if ui.small_button("Apply").clicked() {
                            self.batch_move();
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("rotate to");
                        ui.add(
                            egui::DragValue::new(&mut self.batch_rot)
                                .speed(15.0)
                                .range(-360.0..=360.0)
                                .suffix("°"),
                        );
                        if ui.small_button("Apply").clicked() {
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
                                egui::vec2(ui.available_width(), 190.0),
                                egui::Sense::click_and_drag(),
                            );
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
                            painter.rect_filled(
                                response.rect,
                                4.0,
                                palette::CANVAS,
                            );
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
                                let fill = if selected || pending {
                                    egui::Color32::from_rgb(255, 190, 80)
                                } else {
                                    egui::Color32::from_rgb(70, 100, 140)
                                };
                                painter.rect_filled(*rect, 6.0, fill);
                                painter.text(
                                    rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    id,
                                    egui::FontId::proportional(13.0),
                                    egui::Color32::WHITE,
                                );
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
                        ui.label("Blocks (JSON; add/remove below)");
                        ui.horizontal_wrapped(|ui| {
                            for kind in
                                ["create", "delay", "assign", "decision", "transfer", "sink"]
                            {
                                if ui.small_button(format!("+{kind}")).clicked() {
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
                                for id in ids {
                                    ui.horizontal(|ui| {
                                        ui.label(&id);
                                        if ui.small_button("x").clicked() {
                                            remove = Some(id.clone());
                                        }
                                    });
                                }
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
                        if ui.button("Apply process flow").clicked() {
                            self.apply_flow();
                        }
                    });
                ui.separator();
                panel_heading(ui, "Experiment");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.exp_mode_param, false, "stop-time");
                    ui.radio_value(&mut self.exp_mode_param, true, "parameter");
                });
                if self.exp_mode_param {
                    ui.horizontal(|ui| {
                        ui.label("path");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.exp_path)
                                .hint_text("objects/0/params/process_time/value")
                                .desired_width(240.0),
                        );
                    });
                }
                ui.horizontal(|ui| {
                    ui.label("start");
                    ui.add(egui::DragValue::new(&mut self.exp_start).speed(10.0));
                    ui.label("step");
                    ui.add(egui::DragValue::new(&mut self.exp_step).speed(10.0));
                    ui.label("runs");
                    ui.add(
                        egui::DragValue::new(&mut self.exp_count)
                            .speed(1.0)
                            .range(1..=20),
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
                        painter.rect_filled(bar, 2.0, egui::Color32::from_rgb(90, 150, 210));
                    }
                    egui::Grid::new("exp-grid")
                        .striped(true)
                        .num_columns(3)
                        .show(ui, |ui| {
                            ui.strong("value");
                            ui.strong("completed");
                            ui.strong("± CI95");
                            ui.end_row();
                            for (value, completed, ci) in &self.exp_rows {
                                ui.label(format!("{value:.1}"));
                                ui.label(format!("{completed:.1}"));
                                ui.label(format!("{ci:.1}"));
                                ui.end_row();
                            }
                        });
                }
                ui.separator();
                panel_heading(ui, "Optimise");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.opt_mode, 0, "grid");
                    ui.radio_value(&mut self.opt_mode, 1, "random");
                    ui.radio_value(&mut self.opt_mode, 2, "genetic");
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
                    let columns = if self.opt_secondary_stay { 3 } else { 2 };
                    egui::Grid::new("opt-grid")
                        .striped(true)
                        .num_columns(columns)
                        .show(ui, |ui| {
                            ui.strong("variables");
                            ui.strong("completed");
                            if self.opt_secondary_stay {
                                ui.strong("max stay");
                            }
                            ui.end_row();
                            for (values, score, second) in self.opt_rows.iter().take(12) {
                                ui.label(
                                    values
                                        .iter()
                                        .map(|value| format!("{value:.3}"))
                                        .collect::<Vec<_>>()
                                        .join(" / "),
                                );
                                ui.label(format!("{score:.1}"));
                                if self.opt_secondary_stay {
                                    ui.label(format!("{second:.1}"));
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
                    for (index, (_, score, second)) in self.opt_rows.iter().enumerate() {
                        let x = rect.left()
                            + 6.0
                            + (rect.width() - 12.0) * (*second / max_second) as f32;
                        let y = rect.bottom()
                            - 6.0
                            - (rect.height() - 12.0) * (*score / max_score) as f32;
                        let point = egui::pos2(x, y);
                        let on_front = front.contains(&index);
                        painter.circle_filled(
                            point,
                            if on_front { 4.0 } else { 2.5 },
                            if on_front {
                                egui::Color32::from_rgb(255, 190, 80)
                            } else {
                                egui::Color32::from_rgb(110, 130, 150)
                            },
                        );
                    }
                    ui.label(
                        egui::RichText::new(format!(
                            "Pareto front: {} of {} points (x = max stay, y = completed)",
                            front.len(),
                            self.opt_rows.len()
                        ))
                        .weak()
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
                ui.separator();
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
                                if ui.small_button("x").clicked() {
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
                        ui.radio_value(&mut self.series_metric, 0, "WIP");
                        ui.radio_value(&mut self.series_metric, 1, "throughput");
                        ui.radio_value(&mut self.series_metric, 2, "utilization");
                        if ui.small_button("Export CSV").clicked() {
                            self.export_series_csv();
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for name in &names {
                            if ui
                                .selectable_label(&self.series_object == name, name)
                                .clicked()
                            {
                                self.series_object = name.clone();
                            }
                        }
                    });
                    let (response, painter) = ui.allocate_painter(
                        egui::vec2(ui.available_width(), 120.0),
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
                    for pair in points.windows(2) {
                        painter.line_segment(
                            [pair[0], pair[1]],
                            egui::Stroke::new(1.6_f32, egui::Color32::from_rgb(120, 200, 240)),
                        );
                    }
                    ui.label(
                        egui::RichText::new(format!("{} · max {:.1}", self.series_object, max))
                            .weak()
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
                        ui.label("No checkpoints saved yet.");
                        return;
                    }
                    for (file, time) in self.checkpoint_files.clone() {
                        ui.horizontal(|ui| {
                            let label = if time.is_nan() {
                                file.clone()
                            } else {
                                format!("{file}  t={time:.2}")
                            };
                            if ui.small_button(label).clicked() {
                                self.load_checkpoint_file(&file);
                            }
                        });
                    }
                });
                let mut jump: Option<f64> = None;
                egui::CollapsingHeader::new(format!("Events ({})", self.event_log.len()))
                    .id_salt("event-log")
                    .show(ui, |ui| {
                        if self.event_log.is_empty() {
                            ui.label("Use Step event or Play to fill the log.");
                            return;
                        }
                        egui::ScrollArea::vertical()
                            .id_salt("event-scroll")
                            .max_height(140.0)
                            .stick_to_bottom(true)
                            .show(ui, |ui| {
                                for entry in &self.event_log {
                                    let label =
                                        egui::RichText::new(entry.line()).monospace().small();
                                    if ui.selectable_label(false, label).clicked() {
                                        jump = Some(entry.time);
                                    }
                                }
                            });
                    });
                if let Some(time) = jump {
                    self.jump_to_event(time);
                }
                ui.separator();
                panel_heading(ui, "Report");
                ui.separator();
                if self.report.is_none() {
                    ui.label("Run the model to see statistics.");
                    return;
                }
                let mut export_csv = false;
                let mut export_json = false;
                let mut set_baseline = false;
                let mut export_compare = false;
                let mut save_baseline = false;
                let mut load_baseline = false;
                ui.horizontal(|ui| {
                    if ui.small_button("Export report CSV").clicked() {
                        export_csv = true;
                    }
                    if ui.small_button("Export report JSON").clicked() {
                        export_json = true;
                    }
                    if ui.small_button("Set baseline").clicked() {
                        set_baseline = true;
                    }
                    if ui.small_button("Save baseline").clicked() {
                        save_baseline = true;
                    }
                    if ui.small_button("Load baseline").clicked() {
                        load_baseline = true;
                    }
                    if self.baseline.is_some()
                        && ui.selectable_label(self.show_compare, "Compare").clicked()
                    {
                        self.show_compare = !self.show_compare;
                    }
                    if self.baseline.is_some() && ui.small_button("Export compare CSV").clicked() {
                        export_compare = true;
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
                ui.label(format!(
                    "Completed: {:.1} ± {:.1}   Created: {:.1}",
                    report.completed.mean, report.completed.ci95, report.created.mean
                ));
                ui.separator();
                let (response, painter) = ui.allocate_painter(
                    egui::vec2(ui.available_width(), 130.0),
                    egui::Sense::hover(),
                );
                let rect = response.rect;
                painter.rect_filled(rect, 0.0, palette::CANVAS);
                let entries: Vec<f64> = report
                    .objects
                    .values()
                    .map(|stats| stats.utilization.mean.clamp(0.0, 1.0))
                    .collect();
                if !entries.is_empty() {
                    let width = rect.width() / entries.len() as f32;
                    for (position, utilization) in entries.iter().enumerate() {
                        let height = (rect.height() - 8.0) * (*utilization as f32);
                        let bar = egui::Rect::from_min_max(
                            egui::pos2(
                                rect.left() + position as f32 * width + 2.0,
                                rect.bottom() - 4.0 - height,
                            ),
                            egui::pos2(
                                rect.left() + (position as f32 + 1.0) * width - 2.0,
                                rect.bottom() - 4.0,
                            ),
                        );
                        let color = egui::Color32::from_rgb(
                            (60.0 + 180.0 * *utilization as f32) as u8,
                            140,
                            (200.0 - 120.0 * *utilization as f32) as u8,
                        );
                        painter.rect_filled(bar, 2.0, color);
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
                                ui.strong("Object");
                                ui.strong("In");
                                ui.strong("Out");
                                ui.strong("WIP");
                                ui.strong("Stay");
                                ui.strong("Util");
                                ui.strong("Thru");
                                ui.strong("±CI");
                                ui.end_row();
                                for (name, stats) in &report.objects {
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
                                .show(ui, |ui| {
                                    ui.strong("Object");
                                    ui.strong("Util base");
                                    ui.strong("Util now");
                                    ui.strong("Δ");
                                    ui.end_row();
                                    for (name, current) in &report.objects {
                                        let Some(base) = baseline.objects.get(name) else {
                                            continue;
                                        };
                                        let delta =
                                            current.utilization.mean - base.utilization.mean;
                                        let color = if delta > 0.005 {
                                            egui::Color32::from_rgb(90, 200, 130)
                                        } else if delta < -0.005 {
                                            egui::Color32::from_rgb(220, 100, 90)
                                        } else {
                                            egui::Color32::from_gray(160)
                                        };
                                        ui.label(name);
                                        ui.label(format!("{:.0}%", base.utilization.mean * 100.0));
                                        ui.label(format!(
                                            "{:.0}%",
                                            current.utilization.mean * 100.0
                                        ));
                                        ui.colored_label(color, format!("{:+.1}%", delta * 100.0));
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
                    egui::Grid::new("help-grid").num_columns(2).show(ui, |ui| {
                        for (key, action) in [
                            ("Ctrl/Cmd + O", "open a model"),
                            ("Ctrl/Cmd + N", "new model"),
                            ("Ctrl/Cmd + S", "save"),
                            ("Ctrl/Cmd + Z", "undo"),
                            ("Ctrl/Cmd + Shift + Z", "redo"),
                            ("Space", "play / pause"),
                            ("R", "reset the run"),
                            ("F1", "toggle this dialog"),
                            ("Ctrl/Cmd + A", "select every object"),
                            ("Ctrl/Cmd + D", "duplicate the selection"),
                            ("drag", "rotate the camera"),
                            ("wheel", "zoom"),
                            ("click", "select an object"),
                            ("Right arrow", "step the simulation forward"),
                            ("Up / Down", "increase / decrease the step size"),
                        ] {
                            ui.strong(key);
                            ui.label(action);
                            ui.end_row();
                        }
                    });
                });
            if !open {
                self.show_help = false;
            }
        }
        if self.show_save_as {
            let mut open = true;
            egui::Window::new("Save as")
                .open(&mut open)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Path");
                    ui.add(egui::TextEdit::singleline(&mut self.save_as).desired_width(360.0));
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() {
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
                                    self.status = format!("Saved {}", path.display());
                                    self.show_save_as = false;
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
            if input.consume_key(egui::Modifiers::COMMAND, egui::Key::S) {
                self.save();
            }
            if input.consume_key(egui::Modifiers::COMMAND, egui::Key::O) {
                self.load(PathBuf::from(self.path_input.clone()));
            }
            if input.consume_key(egui::Modifiers::COMMAND, egui::Key::N) {
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
                self.pitch = (self.pitch + delta.y * 0.01).clamp(-1.4, 1.4);
            }
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom * (1.0 + scroll * 0.001)).clamp(0.2, 6.0);
            }
            // Ground grid (z = 0 plane).
            let grid = 20;
            let step = 5.0;
            for line in -grid..=grid {
                let offset = line as f64 * step;
                let extent = grid as f64 * step;
                let color = if line == 0 {
                    egui::Color32::from_rgb(70, 80, 95)
                } else {
                    egui::Color32::from_rgb(38, 44, 54)
                };
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
                painter.line_segment(
                    [start, end],
                    egui::Stroke::new(1.2_f32, egui::Color32::from_rgb(90, 120, 150)),
                );
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
                    egui::Color32::from_rgba_unmultiplied(0, 0, 0, 70),
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
                if selected {
                    fill = egui::Color32::from_rgb(255, 190, 80);
                } else if in_selection {
                    fill = OpenFlexApp::shade(fill, 0.25);
                }
                let rotation = object.rot.unwrap_or(0.0) as f32;
                let box_rect = Self::draw_object_shape(
                    &painter,
                    object.kind,
                    base,
                    self.zoom,
                    fill,
                    selected,
                    rotation,
                    object.z as f32,
                );
                // Flow items held right now: small dots above the box.
                if let Some(stats) = live {
                    let shown = stats.content.min(8) as usize;
                    for slot in 0..shown {
                        let offset = egui::vec2(
                            (slot as f32 - (shown as f32 - 1.0) * 0.5) * (7.0 * self.zoom),
                            -(20.0 * self.zoom),
                        );
                        painter.circle_filled(
                            box_rect.center() + offset,
                            (2.6 * self.zoom).max(1.5),
                            egui::Color32::from_rgb(240, 230, 190),
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
                // Outline first so the caption stays readable on bright
                // heatmap colours (FlexSim draws labels with a halo too).
                let label_pos = box_rect.center_top() - egui::vec2(0.0, 4.0);
                let font = egui::FontId::proportional(12.0);
                for offset in [
                    egui::vec2(-1.0, 0.0),
                    egui::vec2(1.0, 0.0),
                    egui::vec2(0.0, -1.0),
                    egui::vec2(0.0, 1.0),
                ] {
                    painter.text(
                        label_pos + offset,
                        egui::Align2::CENTER_BOTTOM,
                        &caption,
                        font.clone(),
                        egui::Color32::from_rgba_unmultiplied(0, 0, 0, 160),
                    );
                }
                painter.text(
                    label_pos,
                    egui::Align2::CENTER_BOTTOM,
                    &caption,
                    font,
                    egui::Color32::WHITE,
                );
                // State badge: the engine state name in the box corner so the
                // colour is not the only cue (accessibility + FlexSim parity).
                if let Some(name) = live.and_then(|stats| stats.state).map(|state| match state {
                    openflex_core::engine::State::Idle => "idle",
                    openflex_core::engine::State::Busy => "busy",
                    openflex_core::engine::State::Blocked => "blocked",
                    openflex_core::engine::State::Down => "down",
                    openflex_core::engine::State::OffShift => "off",
                }) {
                    painter.text(
                        box_rect.right_bottom() - egui::vec2(3.0, 2.0),
                        egui::Align2::RIGHT_BOTTOM,
                        name,
                        egui::FontId::proportional(9.0),
                        egui::Color32::from_rgba_unmultiplied(255, 255, 255, 190),
                    );
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
                    && box_rect.expand(6.0).contains(pointer)
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
                                let mut copy = model.clone();
                                copy.objects[index].z = 0.0;
                                self.model = Some(Box::leak(Box::new(copy)));
                                self.status = format!("Reset the height of {name}");
                            }
                            self.context_menu_open = None;
                        }
                        if ui.button("Delete object").clicked() {
                            if let Some(model) = self.model {
                                let mut copy = model.clone();
                                copy.objects.retain(|object| object.name != name);
                                copy.connections.retain(|connection| {
                                    connection.from != name && connection.to != name
                                });
                                match copy.validate() {
                                    Ok(()) => {
                                        self.model = Some(Box::leak(Box::new(copy)));
                                        self.selected = None;
                                        self.form = None;
                                        self.status = format!("Deleted {name}");
                                        self.error = None;
                                    }
                                    Err(error) => self.error = Some(error.to_string()),
                                }
                            }
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
                    ui.label(
                        egui::RichText::new(format!(
                            "heatmap: {}",
                            match self.heatmap {
                                1 => "utilization",
                                2 => "throughput",
                                _ => "WIP",
                            }
                        ))
                        .small(),
                    );
                    let green = OpenFlexApp::heat_colour(0.0);
                    let yellow = OpenFlexApp::heat_colour(0.5);
                    let red = OpenFlexApp::heat_colour(1.0);
                    for colour in [green, yellow, red] {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, colour);
                    }
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
        for name in ["mm1", "routing", "batch-queue"] {
            let model = builtin_example(name).unwrap_or_else(|| panic!("{name} missing"));
            model
                .validate()
                .unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(!model.objects.is_empty(), "{name} has no objects");
        }
        assert!(builtin_example("nope").is_none());
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
