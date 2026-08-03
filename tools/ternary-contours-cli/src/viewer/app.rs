use std::sync::mpsc::{Receiver, TryRecvError};

use eframe::egui;

use crate::{ProjectionOptions, RenderOptions};

use super::state::{ViewerState, ViewerStatus, WorkerResult, start_worker};

pub struct LiquidusViewerApp {
    pub(crate) state: ViewerState,
    worker: Option<Receiver<WorkerResult>>,
}

impl LiquidusViewerApp {
    pub fn new(
        input_path: std::path::PathBuf,
        calculation_options: ProjectionOptions,
        render_options: RenderOptions,
    ) -> Self {
        let mut app = Self {
            state: ViewerState::new(input_path, calculation_options, render_options),
            worker: None,
        };
        app.start_calculation();
        app
    }

    pub(crate) fn start_calculation(&mut self) {
        if self.worker.is_some() {
            return;
        }
        let request = self.state.begin_request();
        match start_worker(request) {
            Ok(worker) => self.worker = Some(worker),
            Err(message) => self.state.status = ViewerStatus::Failed(message),
        }
    }

    fn poll_worker(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.worker else {
            return;
        };
        match worker.try_recv() {
            Ok(result) => {
                self.worker = None;
                let accepted = self.state.apply_worker_result(result);
                if !accepted && matches!(self.state.status, ViewerStatus::Calculating) {
                    self.state.status = ViewerStatus::Idle;
                }
                ctx.request_repaint();
            }
            Err(TryRecvError::Disconnected) => {
                self.worker = None;
                self.state.status = ViewerStatus::Failed("calculation worker disconnected".into());
            }
            Err(TryRecvError::Empty) => {
                ctx.request_repaint_after(std::time::Duration::from_millis(80));
            }
        }
    }
}

impl eframe::App for LiquidusViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker(ctx);
        egui::TopBottomPanel::top("viewer_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Liquidus inspection viewer");
                ui.separator();
                ui.label(self.state.input_path.display().to_string());
                ui.separator();
                if ui
                    .add_enabled(self.worker.is_none(), egui::Button::new("Reload"))
                    .clicked()
                {
                    self.start_calculation();
                }
                if ui
                    .add_enabled(self.worker.is_none(), egui::Button::new("Recalculate"))
                    .clicked()
                {
                    self.start_calculation();
                }
            });
        });
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(projection) = self.state.active_projection() {
                ui.heading("Projection ready");
                ui.label(format!(
                    "{} isotherm paths, {} invariants, {} univariants",
                    projection.diagnostics.contour_path_count,
                    projection.diagnostics.invariant_count,
                    projection.diagnostics.univariant_count
                ));
                ui.label("Rendering and inspection controls are loading in the next viewer layer.");
            } else {
                ui.centered_and_justified(|ui| {
                    ui.spinner();
                    ui.label("Parsing and calculating the stable liquidus projection…");
                });
            }
        });
        egui::TopBottomPanel::bottom("viewer_status").show(ctx, |ui| match &self.state.status {
            ViewerStatus::Idle => ui.label("Ready to calculate."),
            ViewerStatus::Calculating => {
                ui.label("Calculation in progress; controls are temporarily disabled.")
            }
            ViewerStatus::Ready => ui.label("Calculation complete."),
            ViewerStatus::Failed(message) => ui.colored_label(egui::Color32::RED, message),
        });
    }
}
