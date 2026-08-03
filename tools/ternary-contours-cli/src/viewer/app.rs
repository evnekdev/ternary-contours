use std::sync::mpsc::{Receiver, TryRecvError};

use eframe::egui;

use crate::{
    OutputFormat, ProjectionOptions, RenderOptions, render_to_bitmap_with_raw,
    render_to_path_with_raw,
};

use super::{
    controls,
    state::{PathDisplayMode, ViewerState, ViewerStatus, WorkerResult, start_worker},
    texture::RenderedTexture,
};

pub struct LiquidusViewerApp {
    pub(crate) state: ViewerState,
    worker: Option<Receiver<WorkerResult>>,
    texture: Option<RenderedTexture>,
    zoom: f32,
    pan: egui::Vec2,
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
            texture: None,
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
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

    fn apply_and_recalculate(&mut self) {
        match controls::apply_calculation_options(&mut self.state) {
            Ok(()) => self.start_calculation(),
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
    fn refresh_texture(&mut self, ctx: &egui::Context) {
        if !self.state.dirty.texture {
            return;
        }
        let rendered = {
            let Some(dataset) = self.state.dataset.as_ref() else {
                return;
            };
            let Some(projection) = self.state.active_projection() else {
                return;
            };
            let raw_projection = matches!(
                self.state.viewer_options.path_display,
                PathDisplayMode::Overlay
            )
            .then_some(self.state.raw_projection.as_ref())
            .flatten();
            render_to_bitmap_with_raw(
                dataset,
                projection,
                raw_projection,
                &self.state.render_options,
            )
        };
        match rendered {
            Ok(bitmap) => {
                let result = match self.texture.as_mut() {
                    Some(texture) => texture.update(bitmap),
                    None => RenderedTexture::from_bitmap(ctx, bitmap).map(|texture| {
                        self.texture = Some(texture);
                    }),
                };
                match result {
                    Ok(()) => {
                        self.state.dirty.render = false;
                        self.state.dirty.texture = false;
                    }
                    Err(message) => self.state.status = ViewerStatus::Failed(message),
                }
            }
            Err(error) => self.state.status = ViewerStatus::Failed(error.to_string()),
        }
    }

    fn export_current(&mut self, format: OutputFormat) {
        let result = {
            let Some(dataset) = self.state.dataset.as_ref() else {
                self.state.status = ViewerStatus::Failed("nothing has been calculated yet".into());
                return;
            };
            let Some(projection) = self.state.active_projection() else {
                self.state.status = ViewerStatus::Failed("nothing has been calculated yet".into());
                return;
            };
            let raw_projection = matches!(
                self.state.viewer_options.path_display,
                PathDisplayMode::Overlay
            )
            .then_some(self.state.raw_projection.as_ref())
            .flatten();
            let extension = match format {
                OutputFormat::Svg => "viewer.svg",
                OutputFormat::Png => "viewer.png",
            };
            let output = self.state.input_path.with_extension(extension);
            render_to_path_with_raw(
                &output,
                dataset,
                projection,
                raw_projection,
                &self.state.render_options,
                Some(format),
            )
            .map(|()| output)
        };
        match result {
            Ok(_output) => self.state.status = ViewerStatus::Ready,
            Err(error) => {
                self.state.status = ViewerStatus::Failed(format!("export failed: {error}"))
            }
        }
    }

    fn show_plot(&mut self, ui: &mut egui::Ui) {
        let Some(texture) = self.texture.as_ref() else {
            ui.centered_and_justified(|ui| {
                if matches!(self.state.status, ViewerStatus::Calculating) {
                    ui.spinner();
                    ui.label("Parsing and calculating the stable liquidus projection…");
                } else {
                    ui.label("A valid projection will appear here after calculation.");
                }
            });
            return;
        };
        let viewport = ui.available_rect_before_wrap();
        if viewport.width() <= 1.0 || viewport.height() <= 1.0 {
            return;
        }
        let response = ui.allocate_rect(viewport, egui::Sense::drag());
        if response.dragged() {
            self.pan += response.drag_delta();
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.smooth_scroll_delta.y);
            if scroll.abs() > f32::EPSILON {
                self.zoom = (self.zoom * (1.0 + scroll * 0.001)).clamp(0.25, 8.0);
            }
        }
        let base = egui::vec2(texture.width as f32, texture.height as f32);
        let fit = (viewport.width() / base.x)
            .min(viewport.height() / base.y)
            .max(0.01);
        let image_rect =
            egui::Rect::from_center_size(viewport.center() + self.pan, base * fit * self.zoom);
        ui.painter_at(viewport).image(
            texture.id(),
            image_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
    }
}

impl eframe::App for LiquidusViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker(ctx);
        self.refresh_texture(ctx);

        let mut fit = false;
        let mut recalculate = false;
        let can_calculate = self.worker.is_none();
        egui::TopBottomPanel::top("viewer_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Liquidus inspection viewer");
                ui.separator();
                if ui
                    .add_enabled(can_calculate, egui::Button::new("Reload"))
                    .clicked()
                {
                    recalculate = true;
                }
                if ui
                    .add_enabled(can_calculate, egui::Button::new("Recalculate"))
                    .clicked()
                {
                    recalculate = true;
                }
                if ui.button("Export SVG").clicked() {
                    self.export_current(OutputFormat::Svg);
                }
                if ui.button("Export PNG").clicked() {
                    self.export_current(OutputFormat::Png);
                }
                if ui.button("Fit").clicked() || ui.button("Reset view").clicked() {
                    fit = true;
                }
            });
        });

        let mut apply_from_controls = false;
        egui::SidePanel::left("viewer_controls")
            .resizable(true)
            .default_width(270.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    apply_from_controls = controls::show(ui, &mut self.state);
                });
            });
        egui::CentralPanel::default().show(ctx, |ui| self.show_plot(ui));
        egui::TopBottomPanel::bottom("viewer_status").show(ctx, |ui| match &self.state.status {
            ViewerStatus::Idle => ui.label("Ready to calculate."),
            ViewerStatus::Calculating => {
                ui.label("Calculation in progress; numerical controls are disabled.")
            }
            ViewerStatus::Ready => ui.label("Calculation complete. Scroll to zoom; drag to pan."),
            ViewerStatus::Failed(message) => ui.colored_label(egui::Color32::RED, message),
        });

        if fit {
            self.zoom = 1.0;
            self.pan = egui::Vec2::ZERO;
        }
        if can_calculate && (recalculate || apply_from_controls) {
            self.apply_and_recalculate();
        }
    }
}
