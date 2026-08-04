use std::{
    path::PathBuf,
    sync::mpsc::{Receiver, TryRecvError},
    time::{Duration, Instant},
};

use eframe::egui;

use crate::{
    DatasetEditorState, OutputFormat, ProjectionOptions, RenderOptions, SourceInterpolation,
    TctSerializeOptions, render_to_bitmap_with_raw, render_to_path_with_raw, save_tct_atomic,
    serialize_tct,
};

use super::{
    controls,
    data_editor::{DataEditorAction, DataEditorUi},
    grid_inspection::{self, GridInspectionAction, GridInspectionUi},
    hit_test::{HitGeometry, NetworkSource, SelectedFeature, ViewerTransform},
    state::{
        CalculationRequest, PathDisplayMode, ViewerState, ViewerStatus, WorkerResult,
        load_tct_dataset, start_worker,
    },
    texture::RenderedTexture,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ViewerTab {
    Data,
    Diagnostics,
    GridInspection,
    Plot,
}

impl ViewerTab {
    const ORDERED: [Self; 4] = [
        Self::Data,
        Self::Diagnostics,
        Self::GridInspection,
        Self::Plot,
    ];

    const fn next(self, backwards: bool) -> Self {
        let index = match self {
            Self::Data => 0,
            Self::Diagnostics => 1,
            Self::GridInspection => 2,
            Self::Plot => 3,
        };
        let index = if backwards {
            (index + Self::ORDERED.len() - 1) % Self::ORDERED.len()
        } else {
            (index + 1) % Self::ORDERED.len()
        };
        Self::ORDERED[index]
    }
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
    grid_inspection_ui: GridInspectionUi,
    tab: ViewerTab,
    sync_editor_on_success: bool,
    pending_request: Option<CalculationRequest>,
    pending_recalculation: Option<Instant>,
    show_plot_after_success: bool,
    open_confirmation: bool,
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
            grid_inspection_ui: GridInspectionUi::default(),
            tab: ViewerTab::Data,
            sync_editor_on_success: false,
            pending_request: None,
            pending_recalculation: None,
            show_plot_after_success: true,
            open_confirmation: false,
        };
        // Initial command-line file startup shares the exact parser, structural
        // validation, editor construction, and Grid inspection initialization
        // used by native Open. Calculation remains on the worker thread.
        match load_tct_dataset(&app.state.input_path) {
            Ok(dataset) => {
                let editor = DatasetEditorState::new(dataset.clone());
                app.grid_inspection_ui.initialise(&editor);
                app.state.dataset = Some(dataset.clone());
                app.editor = Some(editor);
                app.start_dataset_calculation(dataset);
            }
            Err(error) => {
                app.state.status = ViewerStatus::Failed(error.clone());
                app.state.message = Some(format!("File could not be loaded: {error}"));
            }
        }
        app
    }

    pub fn new_default(
        calculation_options: ProjectionOptions,
        render_options: RenderOptions,
        dataset: crate::TabulatedTernaryDataset,
    ) -> Self {
        let editor = DatasetEditorState::new(dataset.clone());
        let mut grid_inspection_ui = GridInspectionUi::default();
        grid_inspection_ui.initialise(&editor);
        Self {
            state: ViewerState::new_unsaved(calculation_options, render_options, dataset),
            worker: None,
            texture: None,
            hit_geometry: HitGeometry::default(),
            zoom: 1.0,
            pan: egui::Vec2::ZERO,
            editor: Some(editor),
            editor_ui: DataEditorUi::default(),
            grid_inspection_ui,
            tab: ViewerTab::Data,
            sync_editor_on_success: false,
            pending_request: None,
            pending_recalculation: None,
            show_plot_after_success: false,
            open_confirmation: false,
        }
    }
    fn start_file_calculation(&mut self) {
        self.sync_editor_on_success = true;
        let request = self.state.begin_request();
        self.queue_request(request);
    }

    fn start_dataset_calculation(&mut self, dataset: crate::TabulatedTernaryDataset) {
        self.sync_editor_on_success = false;
        let request = self.state.begin_dataset_request(dataset);
        self.queue_request(request);
    }

    fn queue_request(&mut self, request: CalculationRequest) {
        if self.worker.is_some() {
            self.pending_request = Some(request);
            self.state.message = Some("Calculating; newer settings pending.".into());
        } else {
            self.launch_request(request);
        }
    }

    fn launch_request(&mut self, request: CalculationRequest) {
        match start_worker(request) {
            Ok(worker) => self.worker = Some(worker),
            Err(message) => {
                self.state.status = ViewerStatus::Failed(message.clone());
                self.state.message = Some(format!("Calculation failed: {message}"));
            }
        }
    }
    fn recalculate_now(&mut self) {
        self.pending_recalculation = None;
        if let Some(editor) = &self.editor {
            self.start_dataset_calculation(editor.active.clone());
        } else {
            self.start_file_calculation();
        }
    }

    /// Coalesce committed calculation changes without ever applying an obsolete
    /// worker result. Render-only and view-only changes never call this method.
    fn schedule_recalculation(&mut self) {
        self.pending_recalculation = Some(Instant::now() + Duration::from_millis(200));
        self.state.status = ViewerStatus::RecalculationPending;
        self.state.message = Some(if self.worker.is_some() {
            "Calculating; newer settings pending.".into()
        } else {
            "Recalculation pending.".into()
        });
    }

    fn launch_debounced_recalculation(&mut self, ctx: &egui::Context) {
        let Some(deadline) = self.pending_recalculation else {
            return;
        };
        if Instant::now() < deadline {
            ctx.request_repaint_after(deadline.saturating_duration_since(Instant::now()));
            return;
        }
        self.recalculate_now();
    }

    fn poll_worker(&mut self, ctx: &egui::Context) {
        let Some(worker) = &self.worker else {
            return;
        };
        match worker.try_recv() {
            Ok(result) => {
                self.worker = None;
                let accepted = self.state.apply_worker_result(result);
                if accepted {
                    self.texture = None;
                    if let Some(dataset) = self.state.dataset.as_ref() {
                        let title = self
                            .state
                            .render_options
                            .title
                            .as_deref()
                            .or(dataset.title.as_deref())
                            .unwrap_or("Untitled ternary system");
                        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.to_owned()));
                    }
                }
                if accepted
                    && self.sync_editor_on_success
                    && let Some(dataset) = self.state.dataset.clone()
                {
                    let editor = DatasetEditorState::new(dataset);
                    self.grid_inspection_ui.initialise(&editor);
                    self.editor = Some(editor);
                    self.editor_ui = DataEditorUi::default();
                }
                self.sync_editor_on_success = false;
                if accepted
                    && matches!(self.state.status, ViewerStatus::Ready)
                    && self.show_plot_after_success
                {
                    self.tab = ViewerTab::Plot;
                    self.show_plot_after_success = false;
                }
                if !accepted && matches!(self.state.status, ViewerStatus::Calculating) {
                    self.state.status = ViewerStatus::Idle;
                }
                if let Some(request) = self.pending_request.take() {
                    self.launch_request(request);
                }
                ctx.request_repaint();
            }
            Err(TryRecvError::Disconnected) => {
                self.worker = None;
                if let Some(request) = self.pending_request.take() {
                    self.launch_request(request);
                } else {
                    self.state.status =
                        ViewerStatus::Failed("calculation worker disconnected".into());
                    self.state.message = Some(
                        "Calculation failed: worker disconnected; displaying the previous valid plot."
                            .into(),
                    );
                }
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
                    Err(message) => {
                        self.state.status = ViewerStatus::Failed(message.clone());
                        self.state.message = Some(format!("Rendering failed: {message}"));
                    }
                }
            }
            Err(error) => {
                let message = error.to_string();
                self.state.status = ViewerStatus::Failed(message.clone());
                self.state.message = Some(format!("Rendering failed: {message}"));
            }
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
                    "path {}: {}…3993 tokens truncated…d samples remain local.",
                        self.state.calculation_options.partial_domain_policy
                    ));
                    if let Some(projection) = self.state.raw_projection.as_ref() {
                        for summary in &projection.diagnostics.partial_cubic_summaries {
                            ui.small(summary);
                        }
                    }
                }
            }
            ui.label("Semantic corner mapping:");
            ui.small(format!(
                "A = {}; B = {}; C = {}",
                dataset.components[0].name, dataset.components[1].name, dataset.components[2].name
            ));
            ui.label("Temperature-source coverage:");
            for grid in &dataset.grids {
                for field in grid.fields().iter().filter(|field| field.property == "T") {
                    let phase_name = dataset
                        .phases
                        .iter()
                        .find(|phase| phase.id == field.phase_id)
                        .map(|phase| phase.name.as_str())
                        .unwrap_or("unknown phase");
                    let mut calculated = 0;
                    let mut non_existing = 0;
                    let mut cut_off = 0;
                    let mut missing = 0;
                    for value in &field.values {
                        match value.state {
                            crate::TabulatedValueState::Calculated => calculated += 1,
                            crate::TabulatedValueState::NonExisting => non_existing += 1,
                            crate::TabulatedValueState::CutOff => cut_off += 1,
                            crate::TabulatedValueState::Missing => missing += 1,
                        }
                    }
                    ui.small(format!(
                        "{} / {phase_name}.T: calculated {calculated}, non-existing {non_existing}, cut-off {cut_off}, missing {missing}",
                        grid.name()
                    ));
                }
            }
        }
        if let Some(message) = &self.state.message {
            ui.colored_label(egui::Color32::RED, message);
            if self.state.raw_projection.is_some() {
                ui.small("The previous valid projection remains displayed.");
            }
            if ui.button("Copy calculation message").clicked() {
                ctx.copy_text(message.clone());
            }
        }
        if let Some(editor) = &self.editor {
            ui.separator();
            ui.label(editor.validation.as_text());
            if ui.button("Copy editor diagnostics").clicked() {
                ctx.copy_text(editor.validation.as_text());
            }
        } else {
            ui.small("Diagnostics will be available after the first successful load.");
        }
        if let Some(projection) = self.state.active_projection() {
            let binary = projection
                .stable_boundaries
                .nodes
                .iter()
                .filter(|node| matches!(node, ternary_contours::StableInvariantNode::Binary(_)))
                .count();
            let interior = projection
                .stable_boundaries
                .nodes
                .iter()
                .filter(|node| matches!(node, ternary_contours::StableInvariantNode::Interior(_)))
                .count();
            ui.separator();
            ui.label(format!(
                "projection: {} stable polygons, {} isotherms, {} invariant nodes ({} binary, {} interior), {} univariants",
                projection.diagnostics.stable_polygon_count,
                projection.diagnostics.contour_path_count,
                projection.diagnostics.invariant_count,
                binary,
                interior,
                projection.diagnostics.univariant_count,
            ));
            ui.label(format!(
                "temperature range: {:.6} to {:.6}",
                projection.input_summary.temperature_range[0],
                projection.input_summary.temperature_range[1],
            ));
        }
    }
    fn show_plot(&mut self, ui: &mut egui::Ui) {
        let Some(texture) = self.texture.as_ref() else {
            ui.centered_and_justified(|ui| match &self.state.status {
                ViewerStatus::Calculating => {
                    ui.spinner();
                    ui.label("Calculating the current dataset...");
                }
                ViewerStatus::Failed(message) => {
                    ui.colored_label(egui::Color32::RED, message);
                    if self.state.raw_projection.is_some() {
                        ui.label("The previous valid projection remains displayed.");
                    }
                }
                _ => {
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
            texture.transform,
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
        if let ViewerStatus::Failed(message) = &self.state.status {
            let overlay = egui::Rect::from_min_size(
                viewport.min + egui::vec2(12.0, 12.0),
                egui::vec2((viewport.width() - 24.0).max(120.0), 58.0),
            );
            painter.rect_filled(overlay, 4.0, egui::Color32::from_black_alpha(190));
            painter.text(
                overlay.left_top() + egui::vec2(10.0, 8.0),
                egui::Align2::LEFT_TOP,
                message,
                egui::FontId::proportional(14.0),
                egui::Color32::LIGHT_RED,
            );
            if self.state.raw_projection.is_some() {
                painter.text(
                    overlay.left_bottom() - egui::vec2(-10.0, 8.0),
                    egui::Align2::LEFT_BOTTOM,
                    "Previous valid projection shown",
                    egui::FontId::proportional(12.0),
                    egui::Color32::WHITE,
                );
            }
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
        let mut open_requested = false;
        let mut save_requested = false;
        let mut save_as_requested = false;
        egui::TopBottomPanel::top("viewer_toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Liquidus inspection viewer");
                ui.separator();
                ui.selectable_value(&mut self.tab, ViewerTab::Data, "Data");
                ui.selectable_value(&mut self.tab, ViewerTab::Diagnostics, "Diagnostics");
                ui.selectable_value(&mut self.tab, ViewerTab::GridInspection, "Grid inspection");
                ui.selectable_value(&mut self.tab, ViewerTab::Plot, "Plot");
                ui.separator();
                open_requested = ui.button("Open").clicked();
                save_requested = ui.button("Save").clicked();
                save_as_requested = ui.button("Save As").clicked();
                let document_label = if self.state.unsaved {
                    "Untitled - unsaved".to_owned()
                } else if self.state.document_dirty
                    || self.editor.as_ref().is_some_and(|editor| editor.dirty)
                {
                    format!("{} - modified", self.state.input_path.display())
                } else {
                    self.state.input_path.display().to_string()
                };
                ui.label(document_label)
                    .on_hover_text("Current document path");
                ui.separator();
                reload = ui
                    .add_enabled(
                        can_calculate && !self.state.unsaved,
                        egui::Button::new("Reload file"),
                    )
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

        let mut control_change = controls::ControlChange::default();
        match self.tab {
            ViewerTab::Plot => {
                egui::SidePanel::left("viewer_controls")
                    .resizable(true)
                    .default_width(270.0)
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            control_change = controls::show(ui, &mut self.state);
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
            ViewerTab::GridInspection => self.show_grid_inspection(ctx),
            ViewerTab::Diagnostics => {
                egui::CentralPanel::default().show(ctx, |ui| self.show_diagnostics_tab(ctx, ui));
            }
        }
        let shortcuts = ctx.input(|input| {
            (
                input.modifiers.command && input.key_pressed(egui::Key::O),
                input.modifiers.command && input.key_pressed(egui::Key::S),
                input.modifiers.command && input.modifiers.shift && input.key_pressed(egui::Key::S),
                input.modifiers.command && input.key_pressed(egui::Key::Tab),
                input.modifiers.shift,
            )
        });
        open_requested |= shortcuts.0;
        save_requested |= shortcuts.1 && !shortcuts.2;
        save_as_requested |= shortcuts.2;
        if shortcuts.3 {
            self.tab = self.tab.next(shortcuts.4);
        }
        if save_requested || save_as_requested {
            self.save_document(ctx, save_as_requested);
        }
        if open_requested {
            if self.has_unsaved_changes() {
                self.open_confirmation = true;
            } else {
                self.begin_open_dialog(ctx);
            }
        }
        if self.open_confirmation {
            let mut save_then_open = false;
            let mut discard_then_open = false;
            let mut cancel = false;
            egui::Window::new("Unsaved changes")
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("The current document has unsaved changes.");
                    ui.horizontal(|ui| {
                        save_then_open = ui.button("Save").clicked();
                        discard_then_open = ui.button("Discard and open").clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                });
            if cancel {
                self.open_confirmation = false;
            } else if save_then_open {
                if self.save_document(ctx, false) {
                    self.open_confirmation = false;
                    self.begin_open_dialog(ctx);
                }
            } else if discard_then_open {
                self.open_confirmation = false;
                self.begin_open_dialog(ctx);
            }
        }

        egui::TopBottomPanel::bottom("viewer_status").show(ctx, |ui| {
            if let Some(message) = &self.state.message {
                ui.label(message);
            } else {
                match &self.state.status {
                    ViewerStatus::Idle => ui.label("Ready to calculate."),
                    ViewerStatus::RecalculationPending => ui.label("Recalculation pending."),
                    ViewerStatus::Calculating => {
                        ui.label("Calculation in progress; numerical controls are disabled.")
                    }
                    ViewerStatus::Ready => {
                        ui.label("Calculation complete. Scroll to zoom; drag to pan.")
                    }
                    ViewerStatus::Failed(message) => ui.colored_label(egui::Color32::RED, message),
                };
            }
        });

        if fit {
            self.zoom = 1.0;
            self.pan = egui::Vec2::ZERO;
        }
        if can_calculate && reload {
            self.start_file_calculation();
        }
        if control_change.calculation_changed {
            self.schedule_recalculation();
        }
        if can_calculate && (recalculate || control_change.recalculate_now) {
            self.recalculate_now();
        }
        self.launch_debounced_recalculation(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_open_replaces_the_starter_dataset_and_initialises_grid_selection() {
        let mut app = LiquidusViewerApp::new_default(
            ProjectionOptions::default(),
            RenderOptions::default(),
            crate::default_regular_dataset(),
        );
        app.state.selection = Some(SelectedFeature::SourceSample {
            grid_index: 0,
            point_index: 0,
        });
        app.state.document_dirty = true;
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/classified-states.tct");
        app.open_document_path(&egui::Context::default(), path.clone())
            .unwrap();
        let dataset = app.editor.as_ref().unwrap();
        assert_eq!(
            dataset.active.title.as_deref(),
            Some("Classified grid states")
        );
        assert_eq!(dataset.active.grids.len(), 1);
        assert_eq!(app.state.dataset.as_ref().unwrap().components[0].name, "A");
        assert_eq!(app.grid_inspection_ui.selected_grid, 0);
        assert_eq!(
            app.grid_inspection_ui.selected_phase,
            Some(ternary_contours::StablePhaseId(1))
        );
        assert_eq!(app.grid_inspection_ui.selected_property, "T");
        assert!(app.state.selection.is_none());
        assert!(!app.state.document_dirty);
        assert_eq!(app.state.input_path, path);
    }

    #[test]
    fn failed_native_open_preserves_the_current_unsaved_editor() {
        let mut app = LiquidusViewerApp::new_default(
            ProjectionOptions::default(),
            RenderOptions::default(),
            crate::default_regular_dataset(),
        );
        app.editor
            .as_mut()
            .unwrap()
            .set_field_point(0, 0, 0, crate::TabulatedValue::calculated(1200.0).unwrap())
            .unwrap();
        let before = app.editor.as_ref().unwrap().draft.clone();
        let missing = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/does-not-exist.tct");
        assert!(
            app.open_document_path(&egui::Context::default(), missing)
                .is_err()
        );
        assert_eq!(app.editor.as_ref().unwrap().draft, before);
        assert!(app.has_unsaved_changes());
    }

    #[test]
    fn tabs_follow_the_document_first_navigation_order() {
        assert_eq!(
            ViewerTab::ORDERED,
            [
                ViewerTab::Data,
                ViewerTab::Diagnostics,
                ViewerTab::GridInspection,
                ViewerTab::Plot,
            ]
        );
        assert_eq!(ViewerTab::Data.next(false), ViewerTab::Diagnostics);
        assert_eq!(
            ViewerTab::Diagnostics.next(false),
            ViewerTab::GridInspection
        );
        assert_eq!(ViewerTab::GridInspection.next(false), ViewerTab::Plot);
        assert_eq!(ViewerTab::Plot.next(false), ViewerTab::Data);
        assert_eq!(ViewerTab::Data.next(true), ViewerTab::Plot);
    }

    #[test]
    fn new_document_starts_on_data_without_a_calculation_request() {
        let app = LiquidusViewerApp::new_default(
            ProjectionOptions::default(),
            RenderOptions::default(),
            crate::default_regular_dataset(),
        );
        assert_eq!(app.tab, ViewerTab::Data);
        assert!(app.worker.is_none());
        assert!(app.pending_recalculation.is_none());
    }

    #[test]
    fn committed_calculation_changes_are_debounced() {
        let mut app = LiquidusViewerApp::new_default(
            ProjectionOptions::default(),
            RenderOptions::default(),
            crate::default_regular_dataset(),
        );
        app.schedule_recalculation();
        assert!(app.pending_recalculation.is_some());
        assert_eq!(app.state.status, ViewerStatus::RecalculationPending);
        assert!(app.worker.is_none());
    }
}

