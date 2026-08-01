//! Backend-independent line contours over regular grids and optional irregular Delaunay meshes.

mod bands;
mod error;
#[cfg(feature = "irregular-delaunay")]
mod irregular;
mod linear;
mod locate;
mod options;
mod paths;
mod regularize;
mod topology;

pub use bands::{ContourBand, ContourBandOptions, ContourBandSet, ContourFragment, ContourRegion};
pub use error::ContourError;
#[cfg(feature = "irregular-delaunay")]
pub use irregular::{
    IrregularAdaptiveContourOptions, IrregularContourDiagnostics, IrregularContourError,
    IrregularContourGeometryOptions, IrregularContourInterpolation,
    IrregularContourLevelDiagnostics, IrregularContourOptions, IrregularContourSet,
    IrregularCubicContourSourceDiagnostics,
};
pub use options::{
    AdaptiveContourOptions, ContourInterpolation, ContourOptions, ContourRegularization,
    CubicAlphaMethod, CubicAlphaOptions, CubicBoundaryPolicy, CubicContourDiagnostics,
};
pub use paths::ContourPath;

use crate::RegularTernaryScalarField;

/// Contours produced for one requested finite scalar level.
#[derive(Clone, Debug, PartialEq)]
pub struct ContourLevel {
    /// Requested finite scalar level.
    pub value: f64,
    /// Deterministically ordered open and closed contour components.
    pub paths: Vec<ContourPath>,
}

/// Backend-independent line contours and optional cubic diagnostics.
///
/// Compute this value before drawing. Paths retain semantic A/B/C compositions and
/// can be converted to plotting coordinates by a separate rendering crate.
#[derive(Clone, Debug, PartialEq)]
pub struct ContourSet {
    /// Contour levels sorted by scalar value.
    pub levels: Vec<ContourLevel>,
    diagnostics: Option<CubicContourDiagnostics>,
}

impl ContourSet {
    /// Compute deterministic contour paths before chart projection or viewport clipping.
    ///
    /// Levels must be finite and distinct within the configured value tolerance.
    /// Linear interpolation is always available; cubic-alpha requests return
    /// [`ContourError::CubicFeatureUnavailable`] unless the `cubic-alpha` feature is enabled.
    pub fn compute(
        field: &RegularTernaryScalarField,
        levels: &[f64],
        options: ContourOptions,
    ) -> Result<Self, ContourError> {
        options.validate()?;
        let levels = validated_levels(levels, options.value_tolerance)?;
        match options.interpolation {
            ContourInterpolation::Linear => {
                let evaluator = regularize::FieldEvaluator::Linear(field);
                let mut result = Vec::with_capacity(levels.len());
                for level in levels {
                    let mut paths = linear::linear_paths(
                        field,
                        level,
                        options.value_tolerance,
                        options.geometry_tolerance,
                    )?;
                    if let Some(regularization) = options.regularization {
                        regularize::regularize_paths(
                            &mut paths,
                            level,
                            regularization,
                            &evaluator,
                        )?;
                    }
                    result.push(ContourLevel {
                        value: level,
                        paths,
                    });
                }
                Ok(Self {
                    levels: result,
                    diagnostics: None,
                })
            }
            ContourInterpolation::CubicAlpha(cubic_options) => {
                #[cfg(not(feature = "cubic-alpha"))]
                {
                    let _ = (field, levels, cubic_options);
                    Err(ContourError::CubicFeatureUnavailable)
                }
                #[cfg(feature = "cubic-alpha")]
                {
                    let mut model = locate::ContourCubicField::new(field, cubic_options)?;
                    let mut result = Vec::with_capacity(levels.len());
                    for level in levels {
                        let mut paths =
                            topology::cubic_paths(&mut model, level, cubic_options.adaptive)?;
                        if let Some(regularization) = options.regularization {
                            let evaluator = regularize::FieldEvaluator::Cubic(&model);
                            if let Err(error) = regularize::regularize_paths(
                                &mut paths,
                                level,
                                regularization,
                                &evaluator,
                            ) {
                                model.diagnostics_mut().projection_failures += 1;
                                return Err(error);
                            }
                        }
                        result.push(ContourLevel {
                            value: level,
                            paths,
                        });
                    }
                    let diagnostics = Some(model.diagnostics().clone());
                    Ok(Self {
                        levels: result,
                        diagnostics,
                    })
                }
            }
        }
    }

    /// Return cubic construction diagnostics, or `None` for linear contours.
    pub fn diagnostics(&self) -> Option<&CubicContourDiagnostics> {
        self.diagnostics.as_ref()
    }
}

fn validated_levels(levels: &[f64], tolerance: f64) -> Result<Vec<f64>, ContourError> {
    let mut indexed = levels.iter().copied().enumerate().collect::<Vec<_>>();
    if let Some((index, value)) = indexed
        .iter()
        .copied()
        .find(|(_, value)| !value.is_finite())
    {
        return Err(ContourError::NonFiniteLevel { index, value });
    }
    indexed.sort_by(|left, right| left.1.total_cmp(&right.1));
    for pair in indexed.windows(2) {
        if (pair[0].1 - pair[1].1).abs() <= tolerance {
            return Err(ContourError::DuplicateLevel {
                first: pair[0].0,
                second: pair[1].0,
                value: pair[0].1,
            });
        }
    }
    Ok(indexed.into_iter().map(|(_, value)| value).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn levels_are_sorted_and_duplicates_or_nonfinite_values_are_rejected() {
        let field = RegularTernaryScalarField::new(1, vec![0.0, 1.0, 2.0]).unwrap();
        let set = ContourSet::compute(&field, &[1.5, 0.5], ContourOptions::linear()).unwrap();
        assert_eq!(
            set.levels
                .iter()
                .map(|level| level.value)
                .collect::<Vec<_>>(),
            vec![0.5, 1.5]
        );
        assert!(matches!(
            ContourSet::compute(&field, &[1.0, 1.0 + 1e-12], ContourOptions::linear()),
            Err(ContourError::DuplicateLevel { .. })
        ));
        assert!(matches!(
            ContourSet::compute(&field, &[f64::NAN], ContourOptions::linear()),
            Err(ContourError::NonFiniteLevel { .. })
        ));
    }
    #[cfg(not(feature = "cubic-alpha"))]
    #[test]
    fn cubic_mode_reports_unavailable_feature() {
        let field = RegularTernaryScalarField::new(1, vec![0.0, 1.0, 2.0]).unwrap();
        assert!(matches!(
            ContourSet::compute(
                &field,
                &[1.0],
                ContourOptions::cubic_alpha(CubicAlphaOptions::default())
            ),
            Err(ContourError::CubicFeatureUnavailable)
        ));
    }
}
