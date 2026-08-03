use std::sync::mpsc::{Receiver, TryRecvError};

use eframe::egui;

use crate::{
    DatasetEditorState, OutputFormat, ProjectionOptions, RenderOptions, render_to_bitmap_with_raw,
    render_to_path_with_raw,
};

use super::{
    controls,
    data_editor::{DataEditorAction, DataEditorUi},
    hit_test::{HitGeometry, NetworkSource, SelectedFeature, ViewerTransform},
    state::{PathDisplayMode, ViewerState, ViewerStatus, WorkerResult, start_worker},
    texture::RenderedTexture,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ViewerTab {
    Plot,
    Data,
    Diagnostics,
}

pub struct LiquidusViewerApp {
    pub(crate) state: ViewerState,
    worker: Option<Receiver<WorkerResult>>,
    texture: Option<RenderedTexture>,
    hit_geometry: HitGeometry,
    zoom: f32,
    pan: egui::Vec2,
    editor: Option<DatasetEditorState>,
    editor_ui: DataEditorUi,
    tab: ViewerTab,
    sync_editor_on_success: bool,
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
            hit_geometry: HitGeometry::default(),
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            editor: None,
            editor_ui: DataEditorUi::default(),
            tab: ViewerTab::Plot,
            sync_editor_on_success: true,
        };
        app.start_calculation();
        app
    }

    pub(crate) fn start_calculation(&mut self) {
        self.start_file_calculation();
    }

    fn start_file_calculation(&mut self) {
        if self.worker.is_some() {
            return;
        }
        self.sync_editor_on_success = true;
        let request = self.state.begin_request();
        self.launch_request(request);
    }

    fn start_dataset_calculation(&mut self, dataset: crate::TabulatedTernaryDataset) {
        if self.worker.is_some() {
            return;
        }
        self.sync_editor_on_success = false;
        let request = self.state.begin_dataset_request(dataset);
        self.launch_request(request);
    }

    fn launch_request(&mut self, request: super::state::CalculationRequest) {
        match start_worker(request) {
            Ok(worker) => self.worker = Some(worker),
            Err(message) => self.state.status = ViewerStatus::Failed(message),
        }
    }
    fn apply_and_recalculate(&mut self) {
        match controls::apply_calculation_options(&mut self.state) {
            Ok(()) => {
                if let Some(editor) = &self.editor {
                    self.start_dataset_calculation(editor.active.clone());
                } else {
                    self.start_file_calculation();
                }
            }
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
                if accepted
                    && self.sync_editor_on_success
                    && let Some(dataset) = self.state.dataset.clone()
                {
                    self.editor = Some(DatasetEditorState::new(dataset));
                    self.editor_ui = DataEditorUi::default();
                }
                self.sync_editor_on_success = false;
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
    fn rebuild_hit_geometry(&mut self) {
        if !self.state.dirty.hit_geometry {
            return;
        }
        let geometry = {
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
            HitGeometry::build(
                dataset,
                projection,
                raw_projection,
                &self.state.render_options,
                self.state.viewer_options.path_display,
            )
        };
        self.hit_geometry = geometry;
        self.state.dirty.hit_geometry = false;
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

    fn show_selection_details(&self, ui: &mut egui::Ui) {
        ui.separator();
        ui.heading("Selection");
        let Some(selection) = self.state.selection.as_ref() else {
            ui.small("Click an invariant, univariant, isotherm, or source point to inspect it.");
            return;
        };
        if ui.button("Copy selection details").clicked() {
            ui.ctx().copy_text(format!("{selection:?}"));
        }
        match selection {
            SelectedFeature::Invariant { node_index } => {
                let Some(node) = self
                    .state
                    .active_projection()
                    .and_then(|projection| projection.stable_boundaries.nodes.get(*node_index))
                else {
                    ui.colored_label(
                        egui::Color32::RED,
                        "Selected invariant is no longer available.",
                    );
                    return;
                };
                ui.label(format!("Invariant node {}", node.id().0));
                ui.label(match node {
                    ternary_contours::StableInvariantNode::Binary(binary) => {
                        format!("binary on {:?}", binary.boundary)
                    }
                    ternary_contours::StableInvariantNode::Interior(_) => "interior".into(),
                });
                ui.label(format!(
                    "composition: {}",
                    format_composition(node.point().as_array())
                ));
                ui.label(format!("temperature: {:.6}", node.temperature()));
                ui.label(format!("phases: {}", self.phase_names(node.phases())));
                let incident = self
                    .state
                    .active_projection()
                    .map(|projection| {
                        projection
                            .stable_boundaries
                            .univariants
                            .iter()
                            .filter(|path| path.start == node.id() || path.end == node.id())
                            .map(|path| path.id.0.to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default();
                ui.label(format!("incident univariants: {incident}"));
            }
            SelectedFeature::Univariant { id, source } => {
                let projection = match source {
                    NetworkSource::Raw => self.state.raw_projection.as_ref(),
                    NetworkSource::Regularized => self
                        .state
                        .regularized_projection
                        .as_ref()
                        .or(self.state.raw_projection.as_ref()),
                };
                let Some(path) = projection.and_then(|projection| {
                    projection
                        .stable_boundaries
                        .univariants
                        .iter()
                        .find(|path| path.id.0 == *id)
                }) else {
                    ui.colored_label(
                        egui::Color32::RED,
                        "Selected univariant is no longer available.",
                    );
                    return;
                };
                ui.label(format!("Univariant {} ({source:?})", path.id.0));
                ui.label(format!(
                    "phase pair: {}",
                    self.phase_names(&[path.phases.first, path.phases.second])
                ));
                ui.label(format!("nodes: {} to {}", path.start.0, path.end.0));
                ui.label(format!("displayed points: {}", path.points.len()));
                if let Some(diagnostics) = &path.regularization {
                    ui.label(format!(
                        "raw/final points: {}/{}; max pair residual: {:.3e}",
                        diagnostics.raw_point_count,
                        diagnostics.final_point_count,
                        diagnostics.maximum_pair_residual
                    ));
                    ui.label(format!(
                        "logical length raw/final: {:.6}/{:.6}",
                        diagnostics.raw_logical_length, diagnostics.final_logical_length
                    ));
                } else {
                    ui.small("raw topology path; no regularization diagnostics");
                }
            }
            SelectedFeature::Isotherm {
                level_index,
                path_index,
                nearest_point,
            } => {
                let Some(level) = self
                    .state
                    .active_projection()
                    .and_then(|projection| projection.stable_contours.levels.get(*level_index))
                else {
                    ui.colored_label(
                        egui::Color32::RED,
                        "Selected isotherm is no longer available.",
                    );
                    return;
                };
                let Some(path) = level.paths.get(*path_index) else {
                    return;
                };
                ui.label(format!(
                    "Isotherm {:.6}, phase {}",
                    level.value,
                    self.phase_names(&[path.phase])
                ));
                ui.label(format!(
                    "path {}: {}",
                    path_index,
                    if path.closed { "closed" } else { "open" }
                ));
                ui.label(format!("point count: {}", path.points.len()));
                if let Some(point) = path.points.get(*nearest_point) {
                    ui.label(format!(
                        "nearest point: {}",
                        format_composition(point.as_array())
                    ));
                }
                ui.small("field residual is not retained by the stable-contour result API.");
            }
            SelectedFeature::SourceSample {
                grid_index,
                point_index,
            } => {
                let Some(grid) = self
                    .state
                    .dataset
                    .as_ref()
                    .and_then(|dataset| dataset.grids.get(*grid_index))
                else {
                    return;
                };
                ui.label(format!("Source sample: {}", grid.name()));
                if let Some(point) = grid.compositions().get(*point_index) {
                    ui.label(format!("composition: {}", format_composition(*point)));
                }
            }
        }
    }

    fn phase_names(&self, phases: &[ternary_contours::StablePhaseId]) -> String {
        phases
            .iter()
            .map(|phase| {
                self.state
                    .dataset
                    .as_ref()
                    .and_then(|dataset| dataset.phases.iter().find(|entry| entry.id == *phase))
                    .map(|entry| entry.name.clone())
                    .unwrap_or_else(|| format!("Phase {}", phase.0))
            })
            .collect::<Vec<_>>()
            .join(", ")
    }
    fn draw_diagnostics(&self, painter: &egui::Painter, transform: &ViewerTransform) {
        let options = &self.state.viewer_options;
        for (points, feature, _closed) in self.hit_geometry.paths() {
            if options.show_path_vertices {
                for point in points {
                    let screen = transform.logical_to_screen(*point);
                    painter.circle_filled(
                        egui::pos2(screen[0] as f32, screen[1] as f32),
                        2.0_f32,
                        egui::Color32::from_rgb(242, 196, 60),
                    );
                }
            }
            let show_endpoints = match feature {
                SelectedFeature::Isotherm { .. } => options.show_contour_endpoints,
                SelectedFeature::Univariant { .. } => options.show_univariant_endpoints,
                _ => false,
            };
            if show_endpoints {
                for point in [points.first(), points.last()].into_iter().flatten() {
                    let screen = transform.logical_to_screen(*point);
                    painter.circle_stroke(
                        egui::pos2(screen[0] as f32, screen[1] as f32),
                        4.0_f32,
                        egui::Stroke::new(1.0_f32, egui::Color32::LIGHT_BLUE),
                    );
                }
            }
            if let SelectedFeature::Univariant { id, source } = feature
                && (options.show_univariant_ids || options.show_phase_pair_labels)
                && let Some(point) = points.get(points.len() / 2)
            {
                let screen = transform.logical_to_screen(*point);
                let label = if options.show_phase_pair_labels {
                    self.phase_pair_label(*id, *source)
                } else {
                    format!("U{id}")
                };
                painter.text(
                    egui::pos2(screen[0] as f32, screen[1] as f32),
                    egui::Align2::LEFT_TOP,
                    label,
                    egui::FontId::monospace(11.0_f32),
                    egui::Color32::DARK_RED,
                );
            }
        }
        if options.show_invariant_ids {
            for (point, feature) in self.hit_geometry.nodes() {
                if let SelectedFeature::Invariant { node_index } = feature {
                    let screen = transform.logical_to_screen(point);
                    painter.text(
                        egui::pos2(screen[0] as f32, screen[1] as f32),
                        egui::Align2::LEFT_BOTTOM,
                        format!("N{node_index}"),
                        egui::FontId::monospace(11.0_f32),
                        egui::Color32::PURPLE,
                    );
                }
            }
        }
    }

    fn phase_pair_label(&self, id: usize, source: NetworkSource) -> String {
        let projection = match source {
            NetworkSource::Raw => self.state.raw_projection.as_ref(),
            NetworkSource::Regularized => self
                .state
                .regularized_projection
                .as_ref()
                .or(self.state.raw_projection.as_ref()),
        };
        projection
            .and_then(|projection| {
                projection
                    .stable_boundaries
                    .univariants
                    .iter()
                    .find(|path| path.id.0 == id)
            })
            .map(|path| self.phase_names(&[path.phases.first, path.phases.second]))
            .unwrap_or_else(|| format!("U{id}"))
    }
    fn show_data(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let dataset = {
            let Some(editor) = self.editor.as_mut() else {
                ui.centered_and_justified(|ui| {
                    ui.label("Load a valid TCT dataset before editing data.")
                });
                return;
            };
            let action = super::data_editor::show(
                ctx,
                ui,
                editor,
                &mut self.editor_ui,
                &self.state.input_path,
            );
            matches!(action, DataEditorAction::Recalculate).then(|| editor.active.clone())
        };
        if let Some(dataset) = dataset {
            self.start_dataset_calculation(dataset);
        }
    }
    fn show_diagnostics_tab(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.heading("Diagnostics");
        if let Some(editor) = &self.editor {
            ui.label(editor.validation.as_text());
            if ui.button("Copy editor diagnostics").clicked() {
                ctx.copy_text(editor.validation.as_text());
            }
        } else {
            ui.small("Diagnostics will be available after the first successful load.");
        }
        if let Some(projection) = self.state.active_projection() {
            ui.separator();
            ui.label(format!(
                "projection: {} isotherms, {} invariant nodes, {} univariants",
                projection.diagnostics.contour_path_count,
                projection.diagnostics.invariant_count,
                projection.diagnostics.univariant_count,
            ));
        }
    }
    fn show_plot(&mut self, ui: &mut egui::Ui) {
        let Some(texture) = self.texture.as_ref() else {
            ui.centered_and_justified(|ui| {
                if matches!(self.state.status, ViewerStatus::Calculating) {
                    ui.spinner();
                    ui.label("Parsing and calculating the stable liquidus projection...");
                } else {
                    ui.label("A valid projection will appear here after calculation.");
                }
            });
            return;
        };
        let (texture_id, bitmap_width, bitmap_height) =
            (texture.id(), texture.width, texture.height);
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
        let base = egui::vec2(bitmap_width as f32, bitmap_height as f32);
        let fit = (viewport.width() / base.x)
            .min(viewport.height() / base.y)
            .max(0.01);
        let image_rect =
            egui::Rect::from_center_size(viewport.center() + self.pan, base * fit * self.zoom);
        let transform = ViewerTransform::new(
            bitmap_width,
            bitmap_height,
            [f64::from(image_rect.min.x), f64::from(image_rect.min.y)],
            [
                f64::from(image_rect.width()),
                f64::from(image_rect.height()),
            ],
        );
        if response.clicked() {
            self.state.selection = response.interact_pointer_pos().and_then(|pointer| {
                self.hit_geometry.hit_test(
                    &transform,
                    [f64::from(pointer.x), f64::from(pointer.y)],
                    10.0,
                )
            });
        }
        let painter = ui.painter_at(viewport);
        painter.image(
            texture_id,
            image_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            egui::Color32::WHITE,
        );
        self.draw_diagnostics(&painter, &transform);
        if let Some(selection) = self.state.selection.as_ref()
            && let Some(anchor) = self.hit_geometry.selected_anchor(selection)
        {
            let screen = transform.logical_to_screen(anchor);
            painter.circle_stroke(
                egui::pos2(screen[0] as f32, screen[1] as f32),
                9.0,
                egui::Stroke::new(2.0_f32, egui::Color32::YELLOW),
            );
        }
    }
}

fn format_composition([a, b, c]: [f64; 3]) -> String {
    format!("({a:.5}, {b:.5}, {c:.5})")
}
impl eframe::App for LiquidusViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_worker(ctx);
        self.rebuild_hit_geometry();
        self.refresh_texture(ctx);

        let mut fit = false;
        let can_calculate = self.worker.is_none();
        let mut reload = false;
        let mut recalculate = false;
        egui::TopBottomPanel::top("viewer_toolbar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("Liquidus inspection viewer");
                ui.separator();
                ui.selectable_value(&mut self.tab, ViewerTab::Plot, "Plot");
                ui.selectable_value(&mut self.tab, ViewerTab::Data, "Data");
                ui.selectable_value(&mut self.tab, ViewerTab::Diagnostics, "Diagnostics");
                ui.separator();
                reload = ui
                    .add_enabled(can_calculate, egui::Button::new("Reload file"))
                    .clicked();
                recalculate = ui
                    .add_enabled(can_calculate, egui::Button::new("Recalculate"))
                    .clicked();
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
        match self.tab {
            ViewerTab::Plot => {
                egui::SidePanel::left("viewer_controls")
                    .resizable(true)
                    .default_width(270.0)
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            apply_from_controls = controls::show(ui, &mut self.state);
                            self.show_selection_details(ui);
                        });
                    });
                egui::CentralPanel::default().show(ctx, |ui| self.show_plot(ui));
            }
            ViewerTab::Data => {
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.show_data(ctx, ui));
                });
            }
            ViewerTab::Diagnostics => {
                egui::CentralPanel::default().show(ctx, |ui| self.show_diagnostics_tab(ctx, ui));
            }
        }
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
        if can_calculate && reload {
            self.start_file_calculation();
        }
        if can_calculate && (recalculate || apply_from_controls) {
            self.apply_and_recalculate();
        }
    }
}
