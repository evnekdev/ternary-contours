use std::{
    path::PathBuf,
    sync::mpsc::{self, Receiver},
    thread,
    time::SystemTime,
};

use super::hit_test::SelectedFeature;

use crate::{
    LiquidusProjection, ProjectionOptions, RenderOptions, RenderPathMode, TabulatedTernaryDataset,
    calculate_projection, parse_path,
};

/// Changes that require numerical work are deliberately separate from texture-only changes.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DirtyFlags {
    pub projection: bool,
    pub render: bool,
    pub texture: bool,
    pub hit_geometry: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ViewerStatus {
    Idle,
    Calculating,
    Ready,
    Failed(String),
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum PathDisplayMode {
    Raw,
    #[default]
    Regularized,
    Overlay,
}

#[derive(Clone, Debug)]
pub struct ViewerOptions {
    pub level_text: String,
    pub regularization_spacing: f64,
    pub path_display: PathDisplayMode,
    pub show_path_vertices: bool,
    pub show_contour_endpoints: bool,
    pub show_univariant_endpoints: bool,
    pub show_invariant_ids: bool,
    pub show_univariant_ids: bool,
    pub show_phase_pair_labels: bool,
}

impl ViewerOptions {
    pub fn from_projection(options: &ProjectionOptions) -> Self {
        Self {
            level_text: options
                .levels
                .iter()
                .map(|level| level.to_string())
                .collect::<Vec<_>>()
                .join(", "),
            regularization_spacing: options.regularization_spacing.unwrap_or(0.02),
            path_display: if options.regularize {
                PathDisplayMode::Regularized
            } else {
                PathDisplayMode::Raw
            },
            show_path_vertices: false,
            show_contour_endpoints: false,
            show_univariant_endpoints: false,
            show_invariant_ids: false,
            show_univariant_ids: false,
            show_phase_pair_labels: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CalculationRequest {
    pub generation: u64,
    pub input: CalculationInput,
    pub options: ProjectionOptions,
}

#[derive(Clone, Debug)]
pub enum CalculationInput {
    Path(PathBuf),
    Dataset(Box<TabulatedTernaryDataset>),
}

#[derive(Clone, Debug)]
pub struct CalculationOutput {
    dataset: TabulatedTernaryDataset,
    raw_projection: LiquidusProjection,
    regularized_projection: Option<LiquidusProjection>,
}

#[derive(Clone, Debug)]
pub enum WorkerResult {
    Ready {
        generation: u64,
        output: Box<CalculationOutput>,
    },
    Failed {
        generation: u64,
        message: String,
    },
}

impl WorkerResult {
    pub const fn generation(&self) -> u64 {
        match self {
            Self::Ready { generation, .. } | Self::Failed { generation, .. } => *generation,
        }
    }
}

/// Spawn an owned parse/validation/calculation request without touching GUI state.
pub fn start_worker(request: CalculationRequest) -> Result<Receiver<WorkerResult>, String> {
    let (sender, receiver) = mpsc::channel();
    thread::Builder::new()
        .name("ternary-contours-viewer-calc".into())
        .spawn(move || {
            let result = calculate_request(&request);
            let message = match result {
                Ok((dataset, raw_projection, regularized_projection)) => WorkerResult::Ready {
                    generation: request.generation,
                    output: Box::new(CalculationOutput {
                        dataset,
                        raw_projection,
                        regularized_projection,
                    }),
                },
                Err(error) => WorkerResult::Failed {
                    generation: request.generation,
                    message: error,
                },
            };
            let _ = sender.send(message);
        })
        .map_err(|error| format!("could not start calculation worker: {error}"))?;
    Ok(receiver)
}

fn calculate_request(
    request: &CalculationRequest,
) -> Result<
    (
        TabulatedTernaryDataset,
        LiquidusProjection,
        Option<LiquidusProjection>,
    ),
    String,
> {
    let dataset = match &request.input {
        CalculationInput::Path(path) => parse_path(path).map_err(|error| error.to_string())?,
        CalculationInput::Dataset(dataset) => dataset.as_ref().clone(),
    };
    let mut raw_options = request.options.clone();
    raw_options.regularize = false;
    let raw_projection =
        calculate_projection(&dataset, &raw_options).map_err(|error| error.to_string())?;
    let regularized_projection = request
        .options
        .regularize
        .then(|| calculate_projection(&dataset, &request.options))
        .transpose()
        .map_err(|error| error.to_string())?;
    Ok((dataset, raw_projection, regularized_projection))
}

/// The persistent model for one viewer window.
#[derive(Clone, Debug)]
pub struct ViewerState {
    pub input_path: PathBuf,
    pub unsaved: bool,
    pub dataset: Option<TabulatedTernaryDataset>,
    pub raw_projection: Option<LiquidusProjection>,
    pub regularized_projection: Option<LiquidusProjection>,
    pub calculation_options: ProjectionOptions,
    pub render_options: RenderOptions,
    pub viewer_options: ViewerOptions,
    pub dirty: DirtyFlags,
    pub status: ViewerStatus,
    pub selection: Option<SelectedFeature>,
    pub last_successful_reload: Option<SystemTime>,
    generation: u64,
}

impl ViewerState {
    pub fn new(
        input_path: PathBuf,
        calculation_options: ProjectionOptions,
        mut render_options: RenderOptions,
    ) -> Self {
        let viewer_options = ViewerOptions::from_projection(&calculation_options);
        render_options.path_mode = match viewer_options.path_display {
            PathDisplayMode::Raw => RenderPathMode::Raw,
            PathDisplayMode::Regularized => RenderPathMode::Regularized,
            PathDisplayMode::Overlay => RenderPathMode::Overlay,
        };
        Self {
            viewer_options,
            input_path,
            unsaved: false,
            dataset: None,
            raw_projection: None,
            regularized_projection: None,
            calculation_options,
            render_options,
            dirty: DirtyFlags {
                projection: true,
                render: true,
                texture: true,
                hit_geometry: true,
            },
            status: ViewerStatus::Idle,
            selection: None,
            last_successful_reload: None,
            generation: 0,
        }
    }

    pub fn new_unsaved(
        calculation_options: ProjectionOptions,
        render_options: RenderOptions,
        dataset: TabulatedTernaryDataset,
    ) -> Self {
        let mut state = Self::new(
            PathBuf::from("Untitled.tct"),
            calculation_options,
            render_options,
        );
        state.dataset = Some(dataset);
        state.unsaved = true;
        state.dirty.projection = false;
        state.status = ViewerStatus::Idle;
        state
    }
    pub fn begin_request(&mut self) -> CalculationRequest {
        self.generation = self.generation.saturating_add(1);
        self.status = ViewerStatus::Calculating;
        self.dirty.projection = false;
        CalculationRequest {
            generation: self.generation,
            input: CalculationInput::Path(self.input_path.clone()),
            options: self.calculation_options.clone(),
        }
    }

    pub fn begin_dataset_request(
        &mut self,
        dataset: TabulatedTernaryDataset,
    ) -> CalculationRequest {
        self.generation = self.generation.saturating_add(1);
        self.status = ViewerStatus::Calculating;
        self.dirty.projection = false;
        CalculationRequest {
            generation: self.generation,
            input: CalculationInput::Dataset(Box::new(dataset)),
            options: self.calculation_options.clone(),
        }
    }
    pub fn invalidate_projection(&mut self) {
        self.generation = self.generation.saturating_add(1);
        self.dirty.projection = true;
        self.dirty.render = true;
        self.dirty.texture = true;
        self.dirty.hit_geometry = true;
        self.selection = None;
        if matches!(self.status, ViewerStatus::Calculating) {
            self.status = ViewerStatus::Idle;
        }
    }

    pub fn mark_render_dirty(&mut self) {
        self.dirty.render = true;
        self.dirty.texture = true;
        self.dirty.hit_geometry = true;
        self.selection = None;
    }

    pub fn active_projection(&self) -> Option<&LiquidusProjection> {
        match self.viewer_options.path_display {
            PathDisplayMode::Raw => self.raw_projection.as_ref(),
            PathDisplayMode::Regularized | PathDisplayMode::Overlay => self
                .regularized_projection
                .as_ref()
                .or(self.raw_projection.as_ref()),
        }
    }

    /// Apply only the newest result. Failed reloads deliberately leave good data intact.
    pub fn apply_worker_result(&mut self, result: WorkerResult) -> bool {
        if result.generation() != self.generation {
            return false;
        }
        match result {
            WorkerResult::Ready { output, .. } => {
                self.dataset = Some(output.dataset);
                self.raw_projection = Some(output.raw_projection);
                self.regularized_projection = output.regularized_projection;
                self.status = ViewerStatus::Ready;
                self.last_successful_reload = Some(SystemTime::now());
                self.dirty.render = true;
                self.dirty.texture = true;
                self.dirty.hit_geometry = true;
                self.selection = None;
            }
            WorkerResult::Failed { message, .. } => {
                self.status = ViewerStatus::Failed(message);
            }
        }
        true
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{calculate_projection, parse_str};

    fn state() -> ViewerState {
        ViewerState::new(
            PathBuf::from("fixture.tct"),
            ProjectionOptions::default(),
            RenderOptions::default(),
        )
    }

    #[test]
    fn dirty_flags_separate_projection_from_render_changes() {
        let mut state = state();
        let _ = state.begin_request();
        assert!(!state.dirty.projection);
        state.mark_render_dirty();
        assert!(!state.dirty.projection);
        assert!(state.dirty.render);
        assert!(state.dirty.texture);
        state.invalidate_projection();
        assert!(state.dirty.projection);
    }

    #[test]
    fn stale_worker_result_is_rejected() {
        let mut state = state();
        let request = state.begin_request();
        state.invalidate_projection();
        assert!(!state.apply_worker_result(WorkerResult::Failed {
            generation: request.generation,
            message: "stale".into(),
        }));
    }

    #[test]
    fn failed_reload_preserves_last_valid_projection() {
        let dataset = parse_str(include_str!("../../fixtures/minimal-regular.tct")).unwrap();
        let projection = calculate_projection(&dataset, &ProjectionOptions::default()).unwrap();
        let mut state = state();
        let first = state.begin_request();
        assert!(state.apply_worker_result(WorkerResult::Ready {
            generation: first.generation,
            output: Box::new(CalculationOutput {
                dataset,
                raw_projection: projection,
                regularized_projection: None,
            }),
        }));
        let second = state.begin_request();
        assert!(state.apply_worker_result(WorkerResult::Failed {
            generation: second.generation,
            message: "malformed row at line 12".into(),
        }));
        assert!(state.dataset.is_some());
        assert!(state.raw_projection.is_some());
        assert!(matches!(state.status, ViewerStatus::Failed(_)));
    }

    #[test]
    fn raw_and_regularized_modes_initialise_shared_render_options() {
        let raw = state();
        assert_eq!(raw.render_options.path_mode, RenderPathMode::Raw);

        let regularized = ViewerState::new(
            PathBuf::from("fixture.tct"),
            ProjectionOptions {
                regularize: true,
                ..ProjectionOptions::default()
            },
            RenderOptions::default(),
        );
        assert_eq!(
            regularized.render_options.path_mode,
            RenderPathMode::Regularized
        );
    }
    #[test]
    fn path_mode_prefers_requested_available_projection() {
        let mut state = state();
        state.viewer_options.path_display = PathDisplayMode::Regularized;
        assert!(state.active_projection().is_none());
    }
}
