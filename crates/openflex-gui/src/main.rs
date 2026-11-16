//! Native desktop GUI for OpenFlex (eframe/egui).
//!
//! The GUI is a thin front end over the existing crates: it loads a `.ofm`
//! model, draws a simple perspective view of the object layout, lets the user
//! move objects, runs the model through `openflex_core::run_model` and shows
//! the per-object statistics of the resulting report.
//!
//! Run with `openflex-gui [model.ofm]`.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui;
use openflex_core::run_model;
use openflex_core::stats::SimReport;
use openflex_model::Model;

fn main() -> eframe::Result<()> {
    let model = std::env::args().nth(1);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("OpenFlex"),
        ..Default::default()
    };
    eframe::run_native(
        "OpenFlex",
        options,
        Box::new(move |_cc| Ok(Box::new(OpenFlexApp::new(model)) as Box<dyn eframe::App>)),
    )
}

struct OpenFlexApp {
    path: Option<PathBuf>,
    model: Option<Model>,
    model_error: Option<String>,
    status: String,
    selected: Option<usize>,
    yaw: f32,
    pitch: f32,
    zoom: f32,
    report: Option<SimReport>,
    running: bool,
    rx: Option<Receiver<Result<SimReport, String>>>,
    position_edit: [f32; 3],
    path_input: String,
}

impl OpenFlexApp {
    fn new(model: Option<String>) -> Self {
        let mut app = Self {
            path: None,
            model: None,
            model_error: None,
            status: "Open a model to start.".into(),
            selected: None,
            yaw: -0.6,
            pitch: 0.5,
            zoom: 1.0,
            report: None,
            running: false,
            rx: None,
            position_edit: [0.0; 3],
            path_input: model.clone().unwrap_or_default(),
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
                self.model_error = Some(format!("could not read {}: {error}", path.display()));
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
                self.path = Some(path);
                self.model = Some(model);
                self.model_error = None;
                self.selected = None;
                self.report = None;
                self.position_edit = [0.0; 3];
            }
            Err(error) => self.model_error = Some(format!("invalid model: {error}")),
        }
    }

    fn save(&mut self) {
        let (Some(path), Some(model)) = (self.path.clone(), self.model.as_ref()) else {
            return;
        };
        match model.to_json() {
            Ok(text) => match std::fs::write(&path, text) {
                Ok(()) => self.status = format!("Saved {}", path.display()),
                Err(error) => self.model_error = Some(format!("could not write: {error}")),
            },
            Err(error) => self.model_error = Some(format!("could not serialise: {error}")),
        }
    }

    fn start_run(&mut self) {
        let Some(model) = self.model.clone() else {
            return;
        };
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.running = true;
        self.status = "Running…".into();
        std::thread::spawn(move || {
            let result = run_model(&model).map_err(|error| error.to_string());
            let _ = tx.send(result);
        });
    }

    fn poll_run(&mut self) {
        let Some(rx) = self.rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(Ok(report)) => {
                self.status = format!(
                    "Done: {:.1} completed per replication, {} objects",
                    report.completed.mean,
                    report.objects.len()
                );
                self.report = Some(report);
                self.running = false;
                self.rx = None;
            }
            Ok(Err(error)) => {
                self.model_error = Some(error);
                self.status = "Run failed".into();
                self.running = false;
                self.rx = None;
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.model_error = Some("simulation thread stopped unexpectedly".into());
                self.running = false;
                self.rx = None;
            }
        }
    }

    fn object_label(model: &Model, index: usize) -> String {
        let object = &model.objects[index];
        format!("{} ({})", object.name, object.kind.as_str())
    }

    /// Screen position of a world point under the current camera.
    fn project(&self, pos: [f64; 2], z: f64, center: egui::Pos2, scale: f32) -> egui::Pos2 {
        let (sin_yaw, cos_yaw) = self.yaw.sin_cos();
        let (sin_pitch, cos_pitch) = self.pitch.sin_cos();
        let x = (pos[0] - 0.0) as f32 * scale;
        let y = (pos[1] - 0.0) as f32 * scale;
        let z = z as f32 * scale;
        // Rotate around z, then tilt.
        let rx = x * cos_yaw - y * sin_yaw;
        let ry = x * sin_yaw + y * cos_yaw;
        let screen_x = rx;
        let screen_y = ry * sin_pitch - z * cos_pitch;
        center + egui::vec2(screen_x, screen_y)
    }
}

impl eframe::App for OpenFlexApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_run();
        if self.running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("OpenFlex");
                ui.separator();
                ui.label("Model:");
                if ui
                    .add(egui::TextEdit::singleline(&mut self.path_input).desired_width(360.0))
                    .lost_focus()
                    && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    && !self.path_input.is_empty()
                {
                    let path = PathBuf::from(self.path_input.clone());
                    self.load(path);
                }
                if ui.button("Open").clicked() && !self.path_input.is_empty() {
                    let path = PathBuf::from(self.path_input.clone());
                    self.load(path);
                }
                if ui.button("Save").clicked() {
                    self.save();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.model.is_some() && !self.running,
                        egui::Button::new("Run"),
                    )
                    .clicked()
                {
                    self.start_run();
                }
                if ui
                    .add_enabled(!self.running, egui::Button::new("Reset view"))
                    .clicked()
                {
                    self.yaw = -0.6;
                    self.pitch = 0.5;
                    self.zoom = 1.0;
                }
                ui.separator();
                ui.label(&self.status);
            });
        });

        if let Some(error) = self.model_error.clone() {
            egui::TopBottomPanel::top("error").show(ctx, |ui| {
                ui.colored_label(egui::Color32::from_rgb(200, 60, 60), error);
            });
        }

        egui::SidePanel::left("objects")
            .resizable(true)
            .default_width(260.0)
            .show(ctx, |ui| {
                ui.heading("Objects");
                ui.separator();
                let Some(model) = self.model.as_ref() else {
                    ui.label("No model loaded.");
                    return;
                };
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() * 0.55)
                    .show(ui, |ui| {
                        for index in 0..model.objects.len() {
                            let label = Self::object_label(model, index);
                            if ui
                                .selectable_label(self.selected == Some(index), label)
                                .clicked()
                            {
                                self.selected = Some(index);
                                let object = &model.objects[index];
                                self.position_edit =
                                    [object.pos[0] as f32, object.pos[1] as f32, object.z as f32];
                            }
                        }
                    });
                ui.separator();
                if let (Some(model), Some(index)) = (self.model.as_mut(), self.selected) {
                    ui.heading("Selected");
                    ui.label(format!("Name: {}", model.objects[index].name));
                    ui.label(format!("Kind: {}", model.objects[index].kind.as_str()));
                    ui.horizontal(|ui| {
                        ui.label("x");
                        ui.add(egui::DragValue::new(&mut self.position_edit[0]).speed(1.0));
                        ui.label("y");
                        ui.add(egui::DragValue::new(&mut self.position_edit[1]).speed(1.0));
                        ui.label("z");
                        ui.add(egui::DragValue::new(&mut self.position_edit[2]).speed(0.1));
                    });
                    if ui.button("Apply position").clicked() {
                        let object = &mut model.objects[index];
                        object.pos = [self.position_edit[0] as f64, self.position_edit[1] as f64];
                        object.z = self.position_edit[2] as f64;
                        self.status = format!("Moved {}", object.name);
                    }
                }
            });

        egui::SidePanel::right("report")
            .resizable(true)
            .default_width(320.0)
            .show(ctx, |ui| {
                ui.heading("Report");
                ui.separator();
                let Some(report) = self.report.as_ref() else {
                    ui.label("Run the model to see statistics.");
                    return;
                };
                ui.label(format!(
                    "Completed: {:.1} ± {:.1}",
                    report.completed.mean, report.completed.ci95
                ));
                ui.label(format!("Created: {:.1}", report.created.mean));
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    egui::Grid::new("stats")
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
            let Some(model) = self.model.as_ref() else {
                ui.centered_and_justified(|ui| ui.label("Open a model to see the layout."));
                return;
            };
            let (response, painter) = ui.allocate_painter(ui.available_size(), egui::Sense::drag());
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

            // Connections first so the objects sit on top of them.
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
                let size = egui::vec2(34.0, 16.0) * self.zoom;
                let box_rect = egui::Rect::from_center_size(base, size);
                let selected = self.selected == Some(index);
                let fill = if selected {
                    egui::Color32::from_rgb(255, 190, 80)
                } else {
                    match object.kind.as_str() {
                        "source" => egui::Color32::from_rgb(70, 150, 100),
                        "sink" => egui::Color32::from_rgb(140, 80, 140),
                        "processor" => egui::Color32::from_rgb(70, 120, 200),
                        "queue" => egui::Color32::from_rgb(90, 160, 190),
                        _ => egui::Color32::from_rgb(120, 130, 140),
                    }
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
                    self.position_edit =
                        [object.pos[0] as f32, object.pos[1] as f32, object.z as f32];
                }
            }
        });
    }
}
