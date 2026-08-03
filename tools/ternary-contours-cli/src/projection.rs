use std::collections::BTreeMap;

use ternary_contours::{
    IrregularTernaryMesh, PathRegularizationOptions, PreparedStablePhaseEnsemble,
    RegularTernaryGrid, StableBoundaryNetwork, StableBoundaryOptions, StableContourQuantity,
    StableContourSet, StableGridOptions, StablePhaseEvaluation, StablePhaseEvaluator,
    StablePhaseId, StablePhaseSource, StablePhaseUndefinedReason, StableScalarSource,
};

use crate::{TabulatedGrid, TabulatedTernaryDataset};

/// Controls for a stable liquidus calculation.
#[derive(Clone, Debug, Default)]
pub struct ProjectionOptions {
    pub levels: Vec<f64>,
    pub sampling_subdivisions: Option<usize>,
    pub regularize: bool,
}

#[derive(Clone, Debug)]
pub struct InputSummary {
    pub phase_count: usize,
    pub regular_grid_count: usize,
    pub irregular_grid_count: usize,
    pub temperature_range: [f64; 2],
}

#[derive(Clone, Debug)]
pub struct ProjectionDiagnostics {
    pub sampling_subdivisions: usize,
    pub regularized: bool,
    pub contour_path_count: usize,
    pub invariant_count: usize,
    pub univariant_count: usize,
}

/// Numerical output shared by terminal reporting and static renderers.
#[derive(Clone, Debug)]
pub struct LiquidusProjection {
    pub levels: Vec<f64>,
    pub stable_contours: StableContourSet,
    pub stable_boundaries: StableBoundaryNetwork,
    pub input_summary: InputSummary,
    pub diagnostics: ProjectionDiagnostics,
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("phase `{phase}` has no temperature field")]
    MissingTemperature { phase: String },
    #[error("could not prepare irregular field for phase {phase:?}: {message}")]
    IrregularMesh {
        phase: StablePhaseId,
        message: String,
    },
    #[error("invalid projection levels: {0}")]
    Levels(String),
    #[error("stable liquidus preparation failed: {0}")]
    Preparation(#[from] ternary_contours::StableContourError),
    #[error("stable boundary calculation failed: {0}")]
    Boundaries(#[from] ternary_contours::StableBoundaryError),
}

enum RuntimeField {
    Regular {
        grid: RegularTernaryGrid,
        values: Vec<Option<f64>>,
    },
    Irregular {
        mesh: Box<IrregularTernaryMesh>,
        values: Vec<Option<f64>>,
    },
}

struct RuntimePhase {
    field: RuntimeField,
}

impl StablePhaseEvaluator for RuntimePhase {
    fn evaluate(&self, composition: [f64; 3]) -> StablePhaseEvaluation {
        let value = match &self.field {
            RuntimeField::Regular { grid, values } => {
                grid.locate(composition).ok().and_then(|location| {
                    location
                        .triangle
                        .vertices
                        .into_iter()
                        .zip(location.barycentric)
                        .try_fold(0.0, |value, (vertex, weight)| {
                            values
                                .get(vertex.0)
                                .copied()
                                .flatten()
                                .map(|sample| value + weight * sample)
                        })
                })
            }
            RuntimeField::Irregular { mesh, values } => {
                mesh.locate(composition).ok().and_then(|location| {
                    location
                        .triangle
                        .vertices
                        .into_iter()
                        .zip(location.barycentric)
                        .try_fold(0.0, |value, (vertex, weight)| {
                            values
                                .get(vertex.0)
                                .copied()
                                .flatten()
                                .map(|sample| value + weight * sample)
                        })
                })
            }
        };
        match value {
            Some(value) if value.is_finite() => StablePhaseEvaluation::Defined { value },
            _ => StablePhaseEvaluation::Undefined {
                reason: StablePhaseUndefinedReason::OutsidePhaseDomain,
            },
        }
    }
}

/// Convert parsed temperature fields to explicit partial-domain evaluators, then
/// calculate stable isotherms and the boundary-connected univariant network.
pub fn calculate_projection(
    dataset: &TabulatedTernaryDataset,
    options: &ProjectionOptions,
) -> Result<LiquidusProjection, ProjectionError> {
    let mut fields = BTreeMap::new();
    let mut regular_grid_count = 0;
    let mut irregular_grid_count = 0;
    let mut extrema = Vec::new();
    for grid in &dataset.grids {
        match grid {
            TabulatedGrid::Regular(_) => regular_grid_count += 1,
            TabulatedGrid::Irregular(_) => irregular_grid_count += 1,
        }
        for field in grid.fields() {
            if field.property != "T" {
                continue;
            }
            extrema.extend(field.values.iter().flatten().copied());
            let runtime = match grid {
                TabulatedGrid::Regular(grid) => RuntimeField::Regular {
                    grid: RegularTernaryGrid::new(grid.subdivisions)
                        .expect("parser validated positive subdivisions"),
                    values: field.values.clone(),
                },
                TabulatedGrid::Irregular(grid) => RuntimeField::Irregular {
                    mesh: Box::new(
                        IrregularTernaryMesh::new(grid.compositions.iter().copied()).map_err(
                            |error| ProjectionError::IrregularMesh {
                                phase: field.phase_id,
                                message: error.to_string(),
                            },
                        )?,
                    ),
                    values: field.values.clone(),
                },
            };
            fields.insert(field.phase_id, runtime);
        }
    }
    let minimum = extrema
        .iter()
        .copied()
        .reduce(f64::min)
        .ok_or_else(|| ProjectionError::Levels("no finite temperature samples".into()))?;
    let maximum = extrema
        .iter()
        .copied()
        .reduce(f64::max)
        .expect("minimum is present");
    let levels = if options.levels.is_empty() {
        default_levels(minimum, maximum)
    } else {
        validate_levels(&options.levels)?;
        options.levels.clone()
    };
    let evaluators = dataset
        .phases
        .iter()
        .map(|phase| {
            let field =
                fields
                    .remove(&phase.id)
                    .ok_or_else(|| ProjectionError::MissingTemperature {
                        phase: phase.name.clone(),
                    })?;
            Ok(RuntimePhase { field })
        })
        .collect::<Result<Vec<_>, ProjectionError>>()?;
    let sources = dataset
        .phases
        .iter()
        .zip(&evaluators)
        .map(|(phase, evaluator)| {
            StablePhaseSource::new(phase.id, StableScalarSource::evaluator(evaluator))
        })
        .collect::<Vec<_>>();
    let sampling_subdivisions = options.sampling_subdivisions.unwrap_or_else(|| {
        dataset
            .grids
            .iter()
            .filter_map(|grid| match grid {
                TabulatedGrid::Regular(grid) => Some(grid.subdivisions),
                TabulatedGrid::Irregular(_) => None,
            })
            .max()
            .unwrap_or(24)
            .max(2)
    });
    if sampling_subdivisions == 0 {
        return Err(ProjectionError::Levels(
            "sampling subdivisions must be positive".into(),
        ));
    }
    let prepared = PreparedStablePhaseEnsemble::new(
        sources,
        StableContourQuantity::Height,
        StableGridOptions {
            subdivisions: sampling_subdivisions,
            ..StableGridOptions::default()
        },
    )?;
    let stable_contours = prepared.contours(&levels)?;
    let stable_boundaries = prepared.stable_boundaries(StableBoundaryOptions {
        regularization: options.regularize.then_some(PathRegularizationOptions {
            spacing: 0.02,
            protected_endpoint_distance: 0.0,
            ..PathRegularizationOptions::default()
        }),
        ..StableBoundaryOptions::default()
    })?;
    let diagnostics = ProjectionDiagnostics {
        sampling_subdivisions,
        regularized: options.regularize,
        contour_path_count: stable_contours
            .levels
            .iter()
            .map(|level| level.paths.len())
            .sum(),
        invariant_count: stable_boundaries.nodes.len(),
        univariant_count: stable_boundaries.univariants.len(),
    };
    Ok(LiquidusProjection {
        levels,
        stable_contours,
        stable_boundaries,
        input_summary: InputSummary {
            phase_count: dataset.phases.len(),
            regular_grid_count,
            irregular_grid_count,
            temperature_range: [minimum, maximum],
        },
        diagnostics,
    })
}

fn default_levels(minimum: f64, maximum: f64) -> Vec<f64> {
    if (maximum - minimum).abs() <= f64::EPSILON {
        vec![minimum]
    } else {
        (1..=5)
            .map(|index| minimum + (maximum - minimum) * index as f64 / 6.0)
            .collect()
    }
}

fn validate_levels(levels: &[f64]) -> Result<(), ProjectionError> {
    if levels.is_empty() || levels.iter().any(|level| !level.is_finite()) {
        return Err(ProjectionError::Levels(
            "levels must be a non-empty finite list".into(),
        ));
    }
    if levels.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(ProjectionError::Levels(
            "levels must be strictly ascending without duplicates".into(),
        ));
    }
    Ok(())
}

/// Parse `800,900,1000` or `800:1400:50` into strictly ascending values.
pub fn parse_level_spec(value: &str) -> Result<Vec<f64>, ProjectionError> {
    if let Some((start, rest)) = value.split_once(':') {
        let (end, step) = rest
            .split_once(':')
            .ok_or_else(|| ProjectionError::Levels("range levels use `start:end:step`".into()))?;
        let start = parse_level(start)?;
        let end = parse_level(end)?;
        let step = parse_level(step)?;
        if step <= 0.0 || end < start {
            return Err(ProjectionError::Levels(
                "range step must be positive and end must not precede start".into(),
            ));
        }
        let mut values = Vec::new();
        let mut next = start;
        while next <= end + step * 1.0e-12 {
            values.push(next.min(end));
            next += step;
            if values.len() > 100_000 {
                return Err(ProjectionError::Levels(
                    "level range has too many entries".into(),
                ));
            }
        }
        validate_levels(&values)?;
        Ok(values)
    } else {
        let values = value
            .split(',')
            .map(parse_level)
            .collect::<Result<Vec<_>, _>>()?;
        validate_levels(&values)?;
        Ok(values)
    }
}

fn parse_level(value: &str) -> Result<f64, ProjectionError> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| ProjectionError::Levels(format!("`{value}` is not a finite number")))
}
