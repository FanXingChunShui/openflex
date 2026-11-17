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
use openflex_model::Model;
use openflex_model::model::{ObjectKind, ObjectParams};

fn main() -> eframe::Result<()> {
    let model = std::env::args().nth(1);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([960.0, 640.0])
            .with_title("OpenFlex"),
        ..Default::default()
    };
    eframe::run_native(
        "OpenFlex",
        options,
        Box::new(move |_cc| Ok(Box::new(OpenFlexApp::new(model)) as Box<dyn eframe::App>)),
    )
}

fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> Result<T, String> {
    serde_json::from_value(value).map_err(|error| error.to_string())
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

struct OpenFlexApp {
    model: Option<&'static Model>,
    path: Option<PathBuf>,
    path_input: String,
    status: String,
    error: Option<String>,
    selected: Option<usize>,
    search: String,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    params_text: String,
    position_edit: [f32; 3],
    new_from: String,
    new_to: String,
    new_port: String,
    handle: Option<SimulationHandle<'static>>,
    sim_state: Option<SimulationState>,
    running: bool,
    paused: bool,
    step_size: f32,
    report: Option<SimReport>,
    report_rx: Option<Receiver<Result<SimReport, String>>>,
    report_running: bool,
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
            search: String::new(),
            yaw: -0.6,
            pitch: 0.5,
            zoom: 1.0,
            params_text: String::new(),
            position_edit: [0.0; 3],
            new_from: String::new(),
            new_to: String::new(),
            new_port: String::new(),
            handle: None,
            sim_state: None,
            running: false,
            paused: false,
            step_size: 10.0,
            report: None,
            report_rx: None,
            report_running: false,
        };
        if let Some(path) = model {
            app.load(PathBuf::from(path));
        }
        app
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
                self.model = Some(Box::leak(Box::new(model)));
                self.path = Some(path.clone());
                self.path_input = path.display().to_string();
                self.error = None;
                self.selected = None;
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
        self.handle = None;
        self.sim_state = None;
        self.running = false;
        self.paused = false;
        self.report = None;
        self.status = "Reset".into();
    }

    fn start_report(&mut self) {
        let Some(model) = self.model else {
            return;
        };
        let (tx, rx) = channel();
        self.report_rx = Some(rx);
        self.report_running = true;
        self.status = "Running a full replication…".into();
        std::thread::spawn(move || {
            let result = run_model(model).map_err(|error| error.to_string());
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

    fn selected_json(&self) -> String {
        let (Some(model), Some(index)) = (self.model, self.selected) else {
            return String::new();
        };
        serde_json::to_string_pretty(&model.objects[index].params).unwrap_or_else(|_| "{}".into())
    }

    fn apply_params(&mut self) {
        let (Some(model), Some(index)) = (self.model, self.selected) else {
            return;
        };
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
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            Err(error) => self.error = Some(error),
        }
    }

    fn add_connection(&mut self) {
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
            kind: None,
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

    fn remove_connection(&mut self, index: usize) {
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
        self.poll_report();
        if self.report_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        if self.running && !self.paused {
            let step = self.step_size.max(0.1) as f64;
            self.advance(step);
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("OpenFlex");
                ui.separator();
                ui.add(egui::TextEdit::singleline(&mut self.path_input).desired_width(280.0));
                if ui.button("Open").clicked() && !self.path_input.is_empty() {
                    self.load(PathBuf::from(self.path_input.clone()));
                }
                if ui.button("Save").clicked() {
                    self.save();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.model.is_some() && !self.report_running,
                        egui::Button::new("Run"),
                    )
                    .clicked()
                {
                    self.start_report();
                }
                if ui
                    .add_enabled(self.model.is_some(), egui::Button::new("Step"))
                    .clicked()
                {
                    self.advance(self.step_size as f64);
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
                if ui
                    .add_enabled(self.model.is_some(), egui::Button::new("Play"))
                    .clicked()
                {
                    self.running = true;
                    self.paused = false;
                }
                if ui.button("Reset").clicked() {
                    self.reset_run();
                }
                ui.separator();
                ui.label("Step");
                ui.add(
                    egui::DragValue::new(&mut self.step_size)
                        .speed(1.0)
                        .range(0.1..=1000.0),
                );
                if let Some(state) = self.sim_state.as_ref() {
                    ui.separator();
                    ui.label(format!("t = {:.2}", state.now));
                    ui.label(format!("created {}", state.items_created));
                    ui.label(format!("completed {}", state.items_completed));
                }
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
            .default_width(300.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Model tree");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.search)
                            .hint_text("filter")
                            .desired_width(110.0),
                    );
                });
                ui.separator();
                let Some(model) = self.model else {
                    ui.label("No model loaded.");
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
                            .show(ui, |ui| {
                                for index in indices {
                                    let object = &model.objects[index];
                                    if ui
                                        .selectable_label(
                                            self.selected == Some(index),
                                            &object.name,
                                        )
                                        .clicked()
                                    {
                                        self.selected = Some(index);
                                        self.position_edit = [
                                            object.pos[0] as f32,
                                            object.pos[1] as f32,
                                            object.z as f32,
                                        ];
                                        self.params_text = self.selected_json();
                                    }
                                }
                            });
                        }
                    });
                ui.separator();
                if let (Some(model), Some(index)) = (self.model, self.selected) {
                    let object = &model.objects[index];
                    ui.horizontal(|ui| {
                        ui.heading(&object.name);
                        ui.label(format!("({})", object.kind.as_str()));
                    });
                    ui.horizontal(|ui| {
                        ui.label("x");
                        ui.add(egui::DragValue::new(&mut self.position_edit[0]).speed(1.0));
                        ui.label("y");
                        ui.add(egui::DragValue::new(&mut self.position_edit[1]).speed(1.0));
                        ui.label("z");
                        ui.add(egui::DragValue::new(&mut self.position_edit[2]).speed(0.1));
                    });
                    if ui.button("Apply position").clicked() {
                        let mut copy = model.clone();
                        copy.objects[index].pos =
                            [self.position_edit[0] as f64, self.position_edit[1] as f64];
                        copy.objects[index].z = self.position_edit[2] as f64;
                        let name = object.name.clone();
                        self.model = Some(Box::leak(Box::new(copy)));
                        self.status = format!("Moved {name}");
                    }
                    ui.label("Parameters (JSON)");
                    ui.add(
                        egui::TextEdit::multiline(&mut self.params_text)
                            .code_editor()
                            .desired_rows(10),
                    );
                    if ui.button("Apply parameters").clicked() {
                        self.apply_params();
                    }
                }
            });

        egui::SidePanel::right("report")
            .resizable(true)
            .default_width(360.0)
            .show(ctx, |ui| {
                ui.heading("Connections");
                ui.separator();
                let Some(model) = self.model else {
                    return;
                };
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
                                ui.label(format!(
                                    "{} → {}{}",
                                    connection.from, connection.to, port
                                ));
                                if ui.small_button("✕").clicked() {
                                    self.remove_connection(index);
                                }
                            });
                        }
                    });
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
                    if ui.button("Add").clicked() {
                        self.add_connection();
                    }
                });
                ui.separator();
                ui.heading("Report");
                ui.separator();
                let Some(report) = self.report.as_ref() else {
                    ui.label("Run the model to see statistics.");
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
                painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(24, 26, 30));
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
                            .num_columns(4)
                            .show(ui, |ui| {
                                ui.strong("Object");
                                ui.strong("In");
                                ui.strong("Out");
                                ui.strong("Util");
                                ui.end_row();
                                for (name, stats) in &report.objects {
                                    ui.label(name);
                                    ui.label(format!("{:.0}", stats.input_count.mean));
                                    ui.label(format!("{:.0}", stats.output_count.mean));
                                    ui.label(format!("{:.0}%", stats.utilization.mean * 100.0));
                                    ui.end_row();
                                }
                            });
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(model) = self.model else {
                ui.centered_and_justified(|ui| ui.label("Open a model to see the layout."));
                return;
            };
            let (response, painter) =
                ui.allocate_painter(ui.available_size(), egui::Sense::drag());
            let rect = response.rect;
            painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(18, 22, 28));
            let center = rect.center();
            let scale = (rect.width().min(rect.height()) / 12.0) * self.zoom;
            if response.dragged() {
                let delta = response.drag_delta();
                self.yaw += delta.x * 0.01;
                self.pitch = (self.pitch + delta.y * 0.01).clamp(-1.4, 1.4);
            }
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.zoom = (self.zoom * (1.0 + scroll * 0.001)).clamp(0.2, 6.0);
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
            for (index, object) in model.objects.iter().enumerate() {
                let base = self.project(object.pos, object.z, center, scale);
                let size = egui::vec2(38.0, 18.0) * self.zoom;
                let box_rect = egui::Rect::from_center_size(base, size);
                let selected = self.selected == Some(index);
                let fill = if selected {
                    egui::Color32::from_rgb(255, 190, 80)
                } else {
                    Self::kind_color(object.kind)
                };
                painter.rect_filled(box_rect, 3.0, fill);
                painter.rect_stroke(
                    box_rect,
                    3.0,
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(20, 24, 30)),
                );
                painter.text(
                    box_rect.center_top() - egui::vec2(0.0, 4.0),
                    egui::Align2::CENTER_BOTTOM,
                    &object.name,
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
                if response.clicked()
                    && let Some(pointer) = response.interact_pointer_pos()
                    && box_rect.expand(6.0).contains(pointer)
                {
                    self.selected = Some(index);
                    self.position_edit = [
                        object.pos[0] as f32,
                        object.pos[1] as f32,
                        object.z as f32,
                    ];
                    self.params_text = self.selected_json();
                }
            }
            ui.label(
                egui::RichText::new(
                    "drag to rotate · wheel to zoom · click an object to edit · Step/Play drive the engine",
                )
                .color(egui::Color32::from_gray(140)),
            );
        });
    }
}
