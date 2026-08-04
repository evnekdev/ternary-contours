use std::collections::BTreeMap;

use ternary_contours::{
    IrregularTernaryMesh, PathRegularizationOptions, PreparedStablePhaseEnsemble,
    RegularTernaryGrid, StableBoundaryNetwork, StableBoundaryOptions, StableContourQuantity,
    StableContourSet, StableGridOptions, StablePhaseEvaluation, StablePhaseEvaluator,
    StablePhaseId, StablePhaseSource, StablePhaseUndefinedReason, StableScalarSource,
};

use crate::{TabulatedGrid, TabulatedTernaryDataset, TabulatedValue, TabulatedValueState};

/// Controls for a stable liquidus calculation.
#[derive(Clone, Debug, Default)]
pub struct ProjectionOptions {
    pub levels: Vec<f64>,
    pub sampling_subdivisions: Option<usize>,
    pub regularize: bool,
    pub regularization_spacing: Option<f64>,
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
    pub stable_polygon_count: usize,
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
    #[error("no calculated finite temperature samples are available: {details}")]
    NoCalculatedTemperatureSamples { details: String },
    #[error("invalid projection levels: {0}")]
    Levels(String),
    #[error(
        "no contour paths were produced because the requested levels do not intersect the calculated temperature range: {minimum} to {maximum} {unit}"
    )]
    NoContourPaths {
        minimum: f64,
        maximum: f64,
        unit: String,
    },
    #[error("stable liquidus preparation failed: {error}; tabulated source coverage: {details}")]
    Preparation {
        error: ternary_contours::StableContourError,
        details: String,
    },
    #[error("stable boundary calculation failed: {0}")]
    Boundaries(#[from] ternary_contours::StableBoundaryError),
}

enum RuntimeField {
    Regular {
        grid: RegularTernaryGrid,
        values: Vec<TabulatedValue>,
    },
    Irregular {
        mesh: Box<IrregularTernaryMesh>,
        values: Vec<TabulatedValue>,
    },
}

struct RuntimePhase {
    field: RuntimeField,
}

impl StablePhaseEvaluator for RuntimePhase {
    fn evaluate(&self, composition: [f64; 3]) -> StablePhaseEvaluation {
        let result = match &self.field {
            RuntimeField::Regular { grid, values } => grid
                .locate(composition)
                .map_err(|_| StablePhaseUndefinedReason::OutsidePhaseDomain)
                .and_then(|location| {
                    interpolate_tabulated(
                        values,
                        location
                            .triangle
                            .vertices
                            .into_iter()
                            .zip(location.barycentric)
                            .map(|(vertex, weight)| (vertex.0, weight)),
                    )
                }),
            RuntimeField::Irregular { mesh, values } => mesh
                .locate(composition)
                .map_err(|_| StablePhaseUndefinedReason::OutsidePhaseDomain)
                .and_then(|location| {
                    interpolate_tabulated(
                        values,
                        location
                            .triangle
                            .vertices
                            .into_iter()
                            .zip(location.barycentric)
                            .map(|(vertex, weight)| (vertex.0, weight)),
                    )
                }),
        };
        match result {
            Ok(value) if value.is_finite() => StablePhaseEvaluation::Defined { value },
            Ok(_) => StablePhaseEvaluation::Undefined {
                reason: StablePhaseUndefinedReason::NonFiniteResult,
            },
            Err(reason) => StablePhaseEvaluation::Undefined { reason },
        }
    }
}

fn interpolate_tabulated(
    values: &[TabulatedValue],
    vertices: impl IntoIterator<Item = (usize, f64)>,
) -> Result<f64, StablePhaseUndefinedReason> {
    vertices
        .into_iter()
        .try_fold(0.0, |interpolated, (index, weight)| {
            let sample = values
                .get(index)
                .ok_or(StablePhaseUndefinedReason::MissingTabulatedInput)?;
            let value = sample
                .calculated_value()
                .ok_or_else(|| undefined_reason(sample))?;
            Ok(interpolated + weight * value)
        })
}

fn undefined_reason(value: &TabulatedValue) -> StablePhaseUndefinedReason {
    match value.state {
        TabulatedValueState::Calculated => StablePhaseUndefinedReason::NonFiniteResult,
        TabulatedValueState::NonExisting => StablePhaseUndefinedReason::ClassifiedNonExisting,
        TabulatedValueState::CutOff => StablePhaseUndefinedReason::ClassifiedCutOff,
        TabulatedValueState::Missing => StablePhaseUndefinedReason::MissingTabulatedInput,
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
    let mut unavailable_fields = Vec::new();
    let mut source_coverage = Vec::new();
    for grid in &dataset.grids {
        match grid {
            TabulatedGrid::Regular(_) => regular_grid_count += 1,
            TabulatedGrid::Irregular(_) => irregular_grid_count += 1,
        }
        for field in grid.fields() {
            if field.property != "T" {
                continue;
            }
            let calculated = field
                .values
                .iter()
                .filter_map(TabulatedValue::calculated_value)
                .collect::<Vec<_>>();
            let counts = field.values.iter().fold([0usize; 4], |mut counts, value| {
                counts[match value.state {
                    TabulatedValueState::Calculated => 0,
                    TabulatedValueState::NonExisting => 1,
                    TabulatedValueState::CutOff => 2,
                    TabulatedValueState::Missing => 3,
                }] += 1;
                counts
            });
            let coverage = format!(
                "grid {} phase {}.{} (calculated {}, non-existing {}, cut-off {}, missing {})",
                grid.name(),
                field.phase_id.0,
                field.property,
                counts[0],
                counts[1],
                counts[2],
                counts[3]
            );
            if calculated.is_empty() {
                unavailable_fields.push(coverage.clone());
            }
            source_coverage.push(coverage);
            extrema.extend(calculated);
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
    let minimum = extrema.iter().copied().reduce(f64::min).ok_or_else(|| {
        ProjectionError::NoCalculatedTemperatureSamples {
            details: unavailable_fields.join("; "),
        }
    })?;
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
    if let Some(spacing) = options.regularization_spacing
        && (!spacing.is_finite() || spacing <= 0.0)
    {
        return Err(ProjectionError::Levels(
            "regularization spacing must be finite and positive".into(),
        ));
    }
    let prepared = PreparedStablePhaseEnsemble::new(
        sources,
        StableContourQuantity::Height,
        StableGridOptions {
            subdivisions: sampling_subdivisions,
            ..StableGridOptions::default()
        },
    )
    .map_err(|error| ProjectionError::Preparation {
        error,
        details: source_coverage.join("; "),
    })?;
    let stable_contours =
        prepared
            .contours(&levels)
            .map_err(|error| ProjectionError::Preparation {
                error,
                details: source_coverage.join("; "),
            })?;
    if stable_contours
        .levels
        .iter()
        .all(|level| level.paths.is_empty())
        && levels
            .iter()
            .all(|level| *level < minimum || *level > maximum)
    {
        return Err(ProjectionError::NoContourPaths {
            minimum,
            maximum,
            unit: dataset
                .property("T")
                .map(|property| property.unit.clone())
                .unwrap_or_default(),
        });
    }
    let stable_boundaries = prepared.stable_boundaries(StableBoundaryOptions {
        regularization: options.regularize.then_some(PathRegularizationOptions {
            spacing: options.regularization_spacing.unwrap_or(0.02),
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
        stable_polygon_count: stable_contours.diagnostics.nonempty_stable_polygons,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_str;

    #[test]
    fn partial_phase_domains_are_supported_without_global_rejection() {
        let dataset = parse_str(include_str!("../fixtures/partial-phase-domain.tct")).unwrap();
        let projection = calculate_projection(&dataset, &ProjectionOptions::default()).unwrap();
        assert_eq!(projection.input_summary.phase_count, 2);
        assert!(projection.input_summary.temperature_range[0].is_finite());
    }

    #[test]
    fn all_declared_temperature_fields_are_in_the_stable_ensemble() {
        let dataset = parse_str(include_str!("../fixtures/minimal-regular.tct")).unwrap();
        let projection = calculate_projection(&dataset, &ProjectionOptions::default()).unwrap();
        assert_eq!(projection.input_summary.phase_count, 3);
        assert!(projection.diagnostics.stable_polygon_count > 0);
    }

    #[test]
    fn classified_missing_coverage_reports_each_temperature_field() {
        let error = calculate_projection(
            &crate::default_regular_dataset(),
            &ProjectionOptions::default(),
        )
        .unwrap_err();
        let message = error.to_string();
        assert!(matches!(
            error,
            ProjectionError::NoCalculatedTemperatureSamples { .. }
        ));
        assert!(message.contains("grid regular phase 1.T"));
        assert!(message.contains("non-existing 0, cut-off 0, missing 66"));
    }

    #[test]
    fn out_of_range_levels_have_an_explicit_empty_contour_error() {
        let dataset = parse_str(include_str!("../fixtures/minimal-regular.tct")).unwrap();
        let error = calculate_projection(
            &dataset,
            &ProjectionOptions {
                levels: vec![1_000.0, 1_100.0],
                ..ProjectionOptions::default()
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ProjectionError::NoContourPaths {
                minimum: 100.0,
                maximum: 120.0,
                ..
            }
        ));
        assert!(
            error
                .to_string()
                .contains("do not intersect the calculated temperature range")
        );
    }
}
