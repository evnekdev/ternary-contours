use std::{
    fs,
    path::PathBuf,
    sync::mpsc::{Receiver, TryRecvError},
    time::{Duration, Instant},
};

use eframe::egui;

use crate::{
    DatasetEditorState, OutputFormat, ProjectionCsvLayerFilter, ProjectionCsvOptions,
    ProjectionOptions, RenderOptions, SourceInterpolation, TctSerializeOptions,
    projection_csv_records, render_to_bitmap_with_raw, render_to_path_with_raw, save_tct_atomic,
    serialize_projection_csv, serialize_tct,
};

use super::{
    contract::{
        self, EventTrace, GuiContractState, RequestId, Revision, UiAction, UiEffect, UiElementId,
        ViewerTab,
    },
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
enum ProjectionExport {
    Svg,
    Png,
    LinesCsv,
}
impl ProjectionExport {
    const fn title(self) -> &'static str {
        match self {
            Self::Svg => "Export Scalable Vector Graphics",
            Self::Png => "Export PNG image",
            Self::LinesCsv => "Export calculated lines CSV",
        }
    }
    const fn filter_name(self) -> &'static str {
        match self {
            Self::Svg => "Scalable Vector Graphics",
            Self::Png => "PNG image",
            Self::LinesCsv => "Comma-separated values",
        }
    }
    const fn extension(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Png => "png",
            Self::LinesCsv => "csv",
        }
    }
    const fn suffix(self) -> &'static str {
        match self {
            Self::Svg | Self::Png => "projection",
            Self::LinesCsv => "lines",
        }
    }
    const fn label(self) -> &'static str {
        match self {
            Self::Svg => "SVG",
            Self::Png => "PNG",
            Self::LinesCsv => "lines CSV",
        }
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
    csv_export_filter: ProjectionCsvLayerFilter,
    contract_state: GuiContractState,
    event_trace: EventTrace,
    contract_projection_request: Option<(RequestId, Revision, Revision)>,
    pending_contract_export: Option<ProjectionExport>,
    open_after_contract_save: bool,
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
            csv_export_filter: ProjectionCsvLayerFilter::default(),
            contract_state: GuiContractState::default(),
            event_trace: EventTrace::default(),
            contract_projection_request: None,
            pending_contract_export: None,
            open_after_contract_save: false,
        };
        // Initial command-line file startup shares the exact parser, structural
        // validation, editor construction, and Grid inspection initialization
        // used by native Open. Calculation remains on the worker thread.
        match load_tct_dataset(&app.state.input_path) {
            Ok(dataset) => {
                let editor = DatasetEditorState::new(dataset.clone());
                app.grid_inspection_ui.initialise(&editor);
                app.state.dataset = Some(dataset.clone());
                app.contract_state.document = contract::DocumentFreshness::LoadedClean;
                app.contract_state.revisions.dataset = Revision(1);
                app.editor = Some(editor);
                app.dispatch_contract(
                    &egui::Context::default(),
                    UiElementId::Recalculate,
                    UiAction::RecalculateRequested,
                );
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
            csv_export_filter: ProjectionCsvLayerFilter::default(),
            contract_state: GuiContractState::default(),
            event_trace: EventTrace::default(),
            contract_projection_request: None,
            pending_contract_export: None,
            open_after_contract_save: false,
        }
    }
    /// Route every migrated interaction through the deterministic reducer before
    /// executing the requested native effect.
    fn dispatch_contract(&mut self, ctx: &egui::Context, element: UiElementId, action: UiAction) {
        if matches!(&action, UiAction::OpenRequested) && self.has_unsaved_changes() {
            self.contract_state.document = contract::DocumentFreshness::Dirty;
            self.contract_state.draft_matches_active = false;
        }
        if let UiAction::ExportRequested(kind) = action {
            self.pending_contract_export = Some(match kind {
                contract::ExportKind::Svg => ProjectionExport::Svg,
                contract::ExportKind::Png => ProjectionExport::Png,
                contract::ExportKind::LinesCsv => ProjectionExport::LinesCsv,
            });
        }
        let effects = contract::dispatch(
            &mut self.contract_state,
            &mut self.event_trace,
            Some(element),
            action,
        );
        self.execute_contract_effects(ctx, effects);
    }

    fn execute_contract_effects(&mut self, ctx: &egui::Context, effects: Vec<UiEffect>) {
        for effect in effects {
            match effect {
                UiEffect::ShowOpenDialog => {
                    let selected = self.choose_open_path();
                    self.dispatch_contract(
                        ctx,
                        UiElementId::OpenDialog,
                        UiAction::OpenDialogCompleted(selected),
                    );
                }
                UiEffect::ShowUnsavedChangesDialog => self.open_confirmation = true,
                UiEffect::ShowSaveDialog => {
                    if let Some(export) = self.pending_contract_export.take() {
                        let selected = self.choose_export_path(export);
                        let kind = match export {
                            ProjectionExport::Svg => contract::ExportKind::Svg,
                            ProjectionExport::Png => contract::ExportKind::Png,
                            ProjectionExport::LinesCsv => contract::ExportKind::LinesCsv,
                        };
                        self.dispatch_contract(
                            ctx,
                            UiElementId::ExportDialog,
                            UiAction::ExportDialogCompleted {
                                kind,
                                path: selected,
                            },
                        );
                    } else {
                        let selected = self.choose_save_path();
                        if selected.is_none() {
                            self.open_after_contract_save = false;
                        }
                        self.dispatch_contract(
                            ctx,
                            UiElementId::SaveDialog,
                            UiAction::SaveDialogCompleted(selected),
                        );
                    }
                }
                UiEffect::LoadDataset { request, path } => {
                    let result = match self.open_document_path(ctx, path.clone()) {
                        Ok(()) => Ok(()),
                        Err(error) => {
                            self.state.message = Some(format!(
                                "File could not be loaded ({}): {error}",
                                path.display()
                            ));
                            Err(contract::UiError(error))
                        }
                    };
                    self.dispatch_contract(
                        ctx,
                        UiElementId::OpenDialog,
                        UiAction::DatasetLoaded { request, result },
                    );
                }
                UiEffect::SaveDataset { request, path } => {
                    let original_path = self.state.input_path.clone();
                    let original_unsaved = self.state.unsaved;
                    self.state.input_path = path;
                    self.state.unsaved = false;
                    let succeeded = self.save_document(ctx, false);
                    if !succeeded {
                        self.state.input_path = original_path;
                        self.state.unsaved = original_unsaved;
                    }
                    self.dispatch_contract(
                        ctx,
                        UiElementId::SaveDialog,
                        UiAction::DatasetSaved {
                            request,
                            result: succeeded
                                .then_some(())
                                .ok_or_else(|| contract::UiError("save failed".into())),
                        },
                    );
                    if succeeded && std::mem::take(&mut self.open_after_contract_save) {
                        self.dispatch_contract(ctx, UiElementId::Open, UiAction::OpenRequested);
                    }
                }
                UiEffect::Export {
                    request,
                    kind,
                    path,
                } => {
                    let succeeded = self.export_to_path(kind, &path);
                    self.dispatch_contract(
                        ctx,
                        UiElementId::ExportDialog,
                        UiAction::ExportCompleted {
                            request,
                            result: succeeded
                                .then_some(())
                                .ok_or_else(|| contract::UiError("export failed".into())),
                        },
                    );
                }
                UiEffect::RecalculateProjection {
                    request,
                    dataset_revision,
                    settings_revision,
                } => {
                    self.contract_projection_request =
                        Some((request, dataset_revision, settings_revision));
                    self.schedule_recalculation();
                }
                UiEffect::RecalculateRegisteredQueries { .. } => {
                    if let Some(editor) = self.editor.as_ref() {
                        self.grid_inspection_ui.recalculate_interpolation_results(
                            editor,
                            &self.state.calculation_options,
                        );
                    }
                }
                UiEffect::RebuildPlotTexture { .. } => self.state.mark_render_dirty(),
                UiEffect::RebuildHitGeometry { .. } => self.state.dirty.hit_geometry = true,
                UiEffect::CopyToClipboard(text) => ctx.copy_text(text),
                UiEffect::PersistWindowLayout(_) => {
                    // Persistence is deliberately deferred until a user chooses
                    // a layout store. The reducer never issues viewport commands.
                }
            }
        }
    }

    fn export_to_path(&mut self, kind: contract::ExportKind, output: &std::path::Path) -> bool {
        match kind {
            contract::ExportKind::Svg | contract::ExportKind::Png => {
                let format = match kind {
                    contract::ExportKind::Svg => OutputFormat::Svg,
                    contract::ExportKind::Png => OutputFormat::Png,
                    contract::ExportKind::LinesCsv => unreachable!(),
                };
                let export = match kind {
                    contract::ExportKind::Svg => ProjectionExport::Svg,
                    contract::ExportKind::Png => ProjectionExport::Png,
                    contract::ExportKind::LinesCsv => unreachable!(),
                };
                let result = match (self.state.dataset.as_ref(), self.state.active_projection()) {
                    (Some(dataset), Some(projection)) => {
                        let raw = matches!(
                            self.state.render_options.path_mode,
                            crate::RenderPathMode::Overlay
                        )
                        .then_some(self.state.raw_projection.as_ref())
                        .flatten();
                        render_to_path_with_raw(
                            output,
                            dataset,
                            projection,
                            raw,
                            &self.state.render_options,
                            Some(format),
                        )
                        .map_err(|error| error.to_string())
                    }
                    _ => Err("There is no calculated projection to export.".into()),
                };
                match result {
                    Ok(()) => {
                        self.state.mark_exported(output);
                        self.state.status = ViewerStatus::Ready;
                        self.state.message = Some(format!(
                            "Exported {}:\n{}",
                            export.label(),
                            output.display()
                        ));
                        true
                    }
                    Err(error) => {
                        self.set_export_error(export, output, &error);
                        false
                    }
                }
            }
            contract::ExportKind::LinesCsv => match self.projection_csv_text() {
                Ok(text) => match fs::write(output, text) {
                    Ok(()) => {
                        self.state.mark_exported(output);
                        self.state.status = ViewerStatus::Ready;
                        self.state.message = Some(format!(
                            "Exported {}:\n{}",
                            ProjectionExport::LinesCsv.label(),
                            output.display()
                        ));
                        true
                    }
                    Err(error) => {
                        self.set_export_error(
                            ProjectionExport::LinesCsv,
                            output,
                            &error.to_string(),
                        );
                        false
                    }
                },
                Err(error) => {
                    self.state.status = ViewerStatus::Failed(error.clone());
                    self.state.message = Some(error);
                    false
                }
            },
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
                let worker_succeeded = matches!(result, WorkerResult::Ready { .. });
                let accepted = self.state.apply_worker_result(result);
                if accepted {
                    if let Some((request, dataset_revision, settings_revision)) =
                        self.contract_projection_request.take()
                    {
                        self.dispatch_contract(
                            ctx,
                            UiElementId::Recalculate,
                            UiAction::ProjectionCalculated {
                                request,
                                dataset_revision,
                                settings_revision,
                                result: worker_succeeded
                                    .then_some(())
                                    .ok_or_else(|| contract::UiError("calculation failed".into())),
                            },
                        );
                    }
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
    fn choose_export_path(&mut self, export: ProjectionExport) -> Option<PathBuf> {
        let editor = self.editor.as_ref()?;
        let document = (!self.state.unsaved).then_some(self.state.input_path.as_path());
        let directory = super::save::default_export_directory(
            self.state.last_export_directory.as_deref(),
            document,
            self.state.last_dialog_directory.as_deref(),
        );
        let filename = super::save::default_projection_filename(
            document,
            editor.draft.title.as_deref(),
            export.suffix(),
            export.extension(),
        );
        let selected = rfd::FileDialog::new()
            .set_title(export.title())
            .add_filter(export.filter_name(), &[export.extension()])
            .set_directory(directory)
            .set_file_name(&filename)
            .save_file();
        let selected = selected?;
        match super::save::ensure_extension(selected.clone(), export.extension()) {
            Ok(path) => Some(path),
            Err(error) => {
                self.state.status = ViewerStatus::Failed(error.clone());
                self.state.message = Some(format!(
                    "Export {} failed:\n{}\n{error}",
                    export.label(),
                    selected.display()
                ));
                None
            }
        }
    }

    fn set_export_error(
        &mut self,
        export: ProjectionExport,
        output: &std::path::Path,
        error: &str,
    ) {
        self.state.status = ViewerStatus::Failed(format!("export failed: {error}"));
        self.state.message = Some(format!(
            "Export {} failed:\n{}\n{error}",
            export.label(),
            output.display()
        ));
    }

    fn projection_csv_text(&self) -> Result<String, String> {
        let dataset = self
            .state
            .dataset
            .as_ref()
            .ok_or_else(|| "There are no calculated lines to export.".to_owned())?;
        let projection = self
            .state
            .regularized_projection
            .as_ref()
            .or(self.state.raw_projection.as_ref());
        let records = projection_csv_records(
            dataset,
            projection,
            self.state.raw_projection.as_ref(),
            &self.state.render_options,
            ProjectionCsvOptions {
                layers: self.csv_export_filter,
                path_mode: self.state.render_options.path_mode,
            },
        )
        .map_err(|error| error.to_string())?;
        serialize_projection_csv(&records).map_err(|error| error.to_string())
    }

    fn copy_lines_csv(&mut self, ctx: &egui::Context) {
        match self.projection_csv_text() {
            Ok(text) => {
                ctx.copy_text(text);
                self.state.message = Some("Copied calculated lines CSV to the clipboard.".into());
            }
            Err(error) => {
                self.state.status = ViewerStatus::Failed(error.clone());
                self.state.message = Some(error);
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
                    let position = egui::pos2(screen[0] as f32, screen[1] as f32);
                    match feature {
                        SelectedFeature::Univariant {
                            source: NetworkSource::Regularized,
                            ..
                        } => painter.rect_filled(
                            egui::Rect::from_center_size(position, egui::vec2(4.0, 4.0)),
                            0.0,
                            egui::Color32::from_rgb(79, 209, 197),
                        ),
                        _ => painter.circle_filled(
                            position,
                            2.0_f32,
                            egui::Color32::from_rgb(242, 196, 60),
                        ),
                    };
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
    fn choose_save_path(&mut self) -> Option<PathBuf> {
        let editor = self.editor.as_ref()?;
        let document = (!self.state.unsaved).then_some(self.state.input_path.as_path());
        let directory = super::save::default_dialog_directory(
            self.state.last_dialog_directory.as_deref(),
            document,
        );
        let filename = super::save::default_filename(
            document,
            self.state.unsaved,
            editor.active.title.as_deref(),
        );
        let selected = rfd::FileDialog::new()
            .set_title("Save Ternary Contour Table")
            .add_filter("Ternary Contour Table", &["tct"])
            .set_directory(directory)
            .set_file_name(&filename)
            .save_file();
        let selected = selected?;
        match super::save::ensure_tct_extension(selected) {
            Ok(path) => Some(path),
            Err(error) => {
                self.state.message = Some(error);
                None
            }
        }
    }

    fn choose_open_path(&mut self) -> Option<PathBuf> {
        let document = (!self.state.unsaved).then_some(self.state.input_path.as_path());
        let directory = super::save::default_dialog_directory(
            self.state.last_dialog_directory.as_deref(),
            document,
        );
        rfd::FileDialog::new()
            .set_title("Open Ternary Contour Table")
            .add_filter("Ternary Contour Table", &["tct"])
            .set_directory(directory)
            .pick_file()
    }

    fn has_unsaved_changes(&self) -> bool {
        self.state.unsaved
            || self.state.document_dirty
            || self.editor.as_ref().is_some_and(|editor| editor.dirty)
    }

    /// Parse and construct all replacement state before touching the current
    /// document. A malformed selection therefore preserves the visible plot and
    /// every pending edit.
    fn open_document_path(&mut self, ctx: &egui::Context, path: PathBuf) -> Result<(), String> {
        let dataset = load_tct_dataset(&path)?;
        let editor = DatasetEditorState::new(dataset.clone());
        let mut grid_inspection_ui = GridInspectionUi::default();
        grid_inspection_ui.initialise(&editor);

        self.state
            .replace_loaded_document(path.clone(), dataset.clone());
        self.editor = Some(editor);
        self.editor_ui = DataEditorUi::default();
        self.grid_inspection_ui = grid_inspection_ui;
        self.texture = None;
        self.hit_geometry = HitGeometry::default();
        self.zoom = 1.0;
        self.pan = egui::Vec2::ZERO;
        self.open_confirmation = false;
        self.tab = ViewerTab::Data;
        self.show_plot_after_success = true;
        self.state.message = Some("Dataset loaded; calculating the new document.".into());
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| format!("Ternary contours - {name}"))
            .unwrap_or_else(|| "Ternary contours liquidus viewer".into());
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
        Ok(())
    }

    fn save_document(&mut self, ctx: &egui::Context, force_dialog: bool) -> bool {
        if let Some(editor) = self.editor.as_mut()
            && editor.dirty
        {
            match editor.apply_draft() {
                Ok(dataset) => {
                    self.state.dataset = Some(dataset);
                    self.state.mark_document_dirty();
                }
                Err(error) => {
                    self.state.message =
                        Some(format!("save failed: draft validation failed: {error}"));
                    return false;
                }
            }
        }
        let path = if super::save::save_requires_dialog(self.state.unsaved, force_dialog) {
            self.choose_save_path()
        } else {
            Some(self.state.input_path.clone())
        };
        let Some(path) = path else {
            return false;
        };
        let Some(editor) = self.editor.as_ref() else {
            self.state.message = Some("no dataset is loaded".into());
            return false;
        };
        let result = serialize_tct(&editor.active, &TctSerializeOptions::default())
            .map_err(|error| error.to_string())
            .and_then(|text| save_tct_atomic(&path, &text).map_err(|error| error.to_string()));
        match result {
            Ok(()) => {
                self.state.mark_saved(path.clone());
                if let Some(editor) = self.editor.as_mut() {
                    editor.active.source_path = Some(path.clone());
                    editor.draft.source_path = Some(path.clone());
                }
                if let Some(dataset) = self.state.dataset.as_mut() {
                    dataset.source_path = Some(path.clone());
                }
                self.state.message = Some(format!("Saved {}", path.display()));
                let title = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| format!("Ternary contours - {name}"))
                    .unwrap_or_else(|| "Ternary contours liquidus viewer".into());
                ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
            }
            Err(error) => {
                self.state.message = Some(format!("save failed: {error}"));
                return false;
            }
        }
        true
    }
    fn show_data(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let (dataset, recalculate_queries) = {
            let Some(editor) = self.editor.as_mut() else {
                ui.centered_and_justified(|ui| {
                    ui.label("Load a valid TCT dataset before editing data.")
                });
                return;
            };
            let action = super::data_editor::show(ctx, ui, editor, &mut self.editor_ui);
            if matches!(
                action,
                DataEditorAction::Applied | DataEditorAction::Recalculate
            ) {
                self.state.mark_document_dirty();
            }
            (
                matches!(action, DataEditorAction::Recalculate).then(|| editor.active.clone()),
                matches!(
                    action,
                    DataEditorAction::Applied | DataEditorAction::Recalculate
                ),
            )
        };
        if recalculate_queries {
            self.dispatch_contract(ctx, UiElementId::DataPasteApply, UiAction::DatasetEdited);
            if let Some(editor) = self.editor.as_ref() {
                self.grid_inspection_ui
                    .recalculate_interpolation_results(editor, &self.state.calculation_options);
            }
        }
        if dataset.is_some() {
            self.state.invalidate_projection();
            self.dispatch_contract(
                ctx,
                UiElementId::Recalculate,
                UiAction::RecalculateRequested,
            );
        }
    }
    fn show_grid_inspection(&mut self, ctx: &egui::Context) {
        let source_before = self.state.calculation_options.source_interpolation;
        let fallback_before = self.state.calculation_options.partial_domain_policy;
        let action = {
            let (editor, grid_ui, options) = (
                &mut self.editor,
                &mut self.grid_inspection_ui,
                &mut self.state.calculation_options,
            );
            if let Some(editor) = editor.as_mut() {
                let mut action = GridInspectionAction::None;
                egui::SidePanel::left("grid_inspection_controls")
                    .resizable(true)
                    .min_width(250.0)
                    .max_width(480.0)
                    .default_width(310.0)
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            action =
                                grid_inspection::show_controls(ctx, ui, editor, grid_ui, options);
                        });
                    });
                action
            } else {
                GridInspectionAction::None
            }
        };
        if matches!(
            action,
            GridInspectionAction::DraftEdited
                | GridInspectionAction::Applied
                | GridInspectionAction::Recalculate
        ) {
            self.state.mark_document_dirty();
            self.dispatch_contract(ctx, UiElementId::GridPointApply, UiAction::DatasetEdited);
        }
        let recalculate = matches!(
            action,
            GridInspectionAction::Applied | GridInspectionAction::Recalculate
        );
        egui::SidePanel::right("grid_inspection_results")
            .resizable(true)
            .min_width(220.0)
            .max_width(620.0)
            .default_width(370.0)
            .show(ctx, |ui| {
                grid_inspection::show_results(ctx, ui, &mut self.grid_inspection_ui);
            });
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(editor) = self.editor.as_ref() {
                grid_inspection::show_canvas(
                    ui,
                    editor,
                    &mut self.grid_inspection_ui,
                    &self.state.calculation_options,
                );
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Load a TCT dataset to inspect grid points.")
                });
            }
        });
        if source_before != self.state.calculation_options.source_interpolation
            || fallback_before != self.state.calculation_options.partial_domain_policy
        {
            self.dispatch_contract(
                ctx,
                UiElementId::GridInterpolation,
                UiAction::CalculationSettingsCommitted,
            );
        } else if recalculate {
            self.state.invalidate_projection();
            self.dispatch_contract(
                ctx,
                UiElementId::Recalculate,
                UiAction::RecalculateRequested,
            );
        }
    }
    fn show_diagnostics_tab(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.heading("Diagnostics");
        if let Some(dataset) = self.state.dataset.as_ref() {
            ui.separator();
            ui.heading("Active numerical configuration");
            let sampling = self
                .state
                .calculation_options
                .sampling_subdivisions
                .unwrap_or(24);
            let sampling_points = (sampling + 1) * (sampling + 2) / 2;
            ui.label(format!(
                "Sampling grid: n = {sampling} ({sampling_points} evaluation vertices)"
            ));
            match self.state.calculation_options.source_interpolation {
                SourceInterpolation::Linear => {
                    ui.label("Source interpolation: Linear (piecewise planar)");
                    ui.small("Cubic model: not selected.");
                }
                SourceInterpolation::CubicAlpha {
                    method,
                    continuation,
                } => {
                    ui.label(format!(
                        "Source interpolation: Cubic alpha ({method:?}; {continuation:?} continuation)"
                    ));
                    ui.small(format!(
                        "Partial-domain cubic fallback: {:?}. Undefined samples remain local.",
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
        ui.separator();
        ui.collapsing("Public state inspector", |ui| {
            ui.monospace(self.contract_state.state_report());
            if contract::button(ui, UiElementId::DiagnosticsCopyState, "Copy state report")
                .clicked()
            {
                ctx.copy_text(self.contract_state.state_report());
            }
        });
        ui.collapsing("GUI event trace", |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for entry in self.event_trace.entries().iter().rev() {
                        ui.small(format!(
                            "{:?} {:?}; effects: {:?}; dataset {} -> {}; settings {} -> {}",
                            entry.element,
                            entry.action,
                            entry.effects,
                            entry.dataset_before.0,
                            entry.dataset_after.0,
                            entry.settings_before.0,
                            entry.settings_after.0,
                        ));
                    }
                });
            ui.horizontal(|ui| {
                if contract::button(ui, UiElementId::DiagnosticsCopyTrace, "Copy trace").clicked() {
                    ctx.copy_text(self.event_trace.as_text());
                }
                if contract::button(ui, UiElementId::DiagnosticsEventTrace, "Clear trace").clicked()
                {
                    self.event_trace.clear();
                }
            });
        });
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
        let mut export_svg_requested = false;
        let mut export_png_requested = false;
        let mut export_csv_requested = false;
        let tab_before = self.tab;
        egui::TopBottomPanel::top("viewer_toolbar").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong("Liquidus inspection viewer");
                ui.separator();
                contract::selectable_value(
                    ui,
                    UiElementId::TabData,
                    &mut self.tab,
                    ViewerTab::Data,
                    "Data",
                );
                contract::selectable_value(
                    ui,
                    UiElementId::TabDiagnostics,
                    &mut self.tab,
                    ViewerTab::Diagnostics,
                    "Diagnostics",
                );
                contract::selectable_value(
                    ui,
                    UiElementId::TabGridInspection,
                    &mut self.tab,
                    ViewerTab::GridInspection,
                    "Grid inspection",
                );
                contract::selectable_value(
                    ui,
                    UiElementId::TabPlot,
                    &mut self.tab,
                    ViewerTab::Plot,
                    "Plot",
                );
                ui.separator();
                open_requested = contract::button(ui, UiElementId::Open, "Open").clicked();
                save_requested = contract::button(ui, UiElementId::Save, "Save").clicked();
                save_as_requested = contract::button(ui, UiElementId::SaveAs, "Save As").clicked();
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
                export_svg_requested =
                    contract::button(ui, UiElementId::ExportSvg, "Export SVG").clicked();
                export_png_requested =
                    contract::button(ui, UiElementId::ExportPng, "Export PNG").clicked();
                ui.separator();
                egui::ComboBox::from_id_salt("projection_csv_layer_filter")
                    .selected_text(self.csv_export_filter.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.csv_export_filter,
                            ProjectionCsvLayerFilter::VisibleCalculatedLayers,
                            ProjectionCsvLayerFilter::VisibleCalculatedLayers.label(),
                        );
                        ui.selectable_value(
                            &mut self.csv_export_filter,
                            ProjectionCsvLayerFilter::AllCalculatedLayers,
                            ProjectionCsvLayerFilter::AllCalculatedLayers.label(),
                        );
                        ui.selectable_value(
                            &mut self.csv_export_filter,
                            ProjectionCsvLayerFilter::StableIsothermsOnly,
                            ProjectionCsvLayerFilter::StableIsothermsOnly.label(),
                        );
                        ui.selectable_value(
                            &mut self.csv_export_filter,
                            ProjectionCsvLayerFilter::StableUnivariantsOnly,
                            ProjectionCsvLayerFilter::StableUnivariantsOnly.label(),
                        );
                        ui.selectable_value(
                            &mut self.csv_export_filter,
                            ProjectionCsvLayerFilter::InvariantsOnly,
                            ProjectionCsvLayerFilter::InvariantsOnly.label(),
                        );
                    });
                export_csv_requested =
                    contract::button(ui, UiElementId::ExportLinesCsv, "Export lines CSV").clicked();
                if contract::button(ui, UiElementId::CopyLinesCsv, "Copy lines CSV").clicked() {
                    self.copy_lines_csv(ctx);
                }
                if contract::button(ui, UiElementId::Fit, "Fit").clicked()
                    || contract::button(ui, UiElementId::ResetView, "Reset view").clicked()
                {
                    fit = true;
                }
            });
        });

        let mut control_change = controls::ControlChange::default();
        match self.tab {
            ViewerTab::Plot => {
                egui::SidePanel::left("viewer_controls")
                    .resizable(true)
                    .min_width(250.0)
                    .max_width(480.0)
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
                egui::CentralPanel::default().show(ctx, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| self.show_diagnostics_tab(ctx, ui));
                });
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
        if self.tab != tab_before {
            self.dispatch_contract(ctx, UiElementId::TabBar, UiAction::TabSelected(self.tab));
        }
        if save_requested {
            self.dispatch_contract(ctx, UiElementId::Save, UiAction::SaveRequested);
        }
        if save_as_requested {
            self.dispatch_contract(ctx, UiElementId::SaveAs, UiAction::SaveAsRequested);
        }
        if export_svg_requested {
            self.dispatch_contract(
                ctx,
                UiElementId::ExportSvg,
                UiAction::ExportRequested(contract::ExportKind::Svg),
            );
        }
        if export_png_requested {
            self.dispatch_contract(
                ctx,
                UiElementId::ExportPng,
                UiAction::ExportRequested(contract::ExportKind::Png),
            );
        }
        if export_csv_requested {
            self.dispatch_contract(
                ctx,
                UiElementId::ExportLinesCsv,
                UiAction::ExportRequested(contract::ExportKind::LinesCsv),
            );
        }
        if open_requested {
            self.dispatch_contract(ctx, UiElementId::Open, UiAction::OpenRequested);
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
                        save_then_open = contract::button(ui, UiElementId::Save, "Save").clicked();
                        discard_then_open =
                            contract::button(ui, UiElementId::Open, "Discard and open").clicked();
                        cancel = contract::button(ui, UiElementId::UnsavedChangesDialog, "Cancel")
                            .clicked();
                    });
                });
            if cancel {
                self.open_confirmation = false;
                self.dispatch_contract(
                    ctx,
                    UiElementId::UnsavedChangesDialog,
                    UiAction::UnsavedDecisionSelected(contract::UnsavedDecision::Cancel),
                );
            } else if save_then_open {
                self.open_confirmation = false;
                self.open_after_contract_save = true;
                self.dispatch_contract(
                    ctx,
                    UiElementId::UnsavedChangesDialog,
                    UiAction::UnsavedDecisionSelected(contract::UnsavedDecision::Save),
                );
            } else if discard_then_open {
                self.open_confirmation = false;
                self.dispatch_contract(
                    ctx,
                    UiElementId::UnsavedChangesDialog,
                    UiAction::UnsavedDecisionSelected(contract::UnsavedDecision::Discard),
                );
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
            self.dispatch_contract(
                ctx,
                UiElementId::PlotInterpolation,
                UiAction::CalculationSettingsCommitted,
            );
        }
        if can_calculate && (recalculate || control_change.recalculate_now) {
            self.dispatch_contract(
                ctx,
                UiElementId::Recalculate,
                UiAction::RecalculateRequested,
            );
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
    #[test]
    fn export_kinds_use_the_native_dialog_extensions_and_suggested_suffixes() {
        assert_eq!(ProjectionExport::Svg.extension(), "svg");
        assert_eq!(ProjectionExport::Png.extension(), "png");
        assert_eq!(ProjectionExport::LinesCsv.extension(), "csv");
        assert_eq!(ProjectionExport::Svg.suffix(), "projection");
        assert_eq!(ProjectionExport::Png.suffix(), "projection");
        assert_eq!(ProjectionExport::LinesCsv.suffix(), "lines");
        assert_eq!(
            super::super::save::default_projection_filename(
                Some(std::path::Path::new("CaO-PbO-ZnO.tct")),
                None,
                ProjectionExport::Svg.suffix(),
                ProjectionExport::Svg.extension(),
            ),
            "CaO-PbO-ZnO-projection.svg"
        );
    }
}
