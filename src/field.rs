//! Construction of cubic-alpha fields on a regular ternary lattice.

#[cfg(feature = "cubic-alpha")]
use std::collections::BTreeMap;

use crate::{
    FieldError, RegularTernaryScalarField,
    grid::GridTriangle,
    interpolation::{CubicAlphaBuildOptions, CubicAlphaTriangle},
};
#[cfg(feature = "cubic-alpha")]
use crate::{
    GridVertexId,
    grid::{GridEdgeKey, LatticeCoordinate},
    interpolation::{AlphaInterval, CubicAlphaMethod, CubicBoundaryPolicy, DirectedAlphaInterval},
};

/// Counts produced while deriving shared regular-grid cubic intervals.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CubicBuildDiagnostics {
    /// Number of edges assigned an interval from a three-or-more-sample line.
    pub cubic_edges: usize,
    /// Number of two-sample lines represented by a zero-alpha interval.
    pub linear_fallback_edges: usize,
}

/// Precomputed local cubic-alpha fields for every regular-grid elementary triangle.
///
/// This is an advanced numerical type. It has no rendering, viewport, or contour
/// path-extraction dependency.
pub struct CubicGridField<'a> {
    field: &'a RegularTernaryScalarField,
    triangles: Vec<GridTriangle>,
    models: Vec<CubicAlphaTriangle>,
    diagnostics: CubicBuildDiagnostics,
}

impl<'a> CubicGridField<'a> {
    /// Build one shared-edge cubic-alpha model for each elementary triangle.
    pub fn new(
        field: &'a RegularTernaryScalarField,
        options: CubicAlphaBuildOptions,
    ) -> Result<Self, FieldError> {
        #[cfg(not(feature = "cubic-alpha"))]
        {
            let _ = (field, options);
            return Err(FieldError::CubicFeatureUnavailable);
        }
        #[cfg(feature = "cubic-alpha")]
        {
            let mut diagnostics = CubicBuildDiagnostics::default();
            let intervals = build_edge_intervals(field, options, &mut diagnostics)?;
            let triangles = field.elementary_triangles()?;
            let mut models = Vec::with_capacity(triangles.len());
            for triangle in &triangles {
                let [v0, v1, v2] = triangle.vertices;
                let values = [field.value(v0)?, field.value(v1)?, field.value(v2)?];
                let pairs = [(0, 1), (1, 2), (0, 2)];
                let mut directed = Vec::with_capacity(3);
                for (left, right) in pairs {
                    let key = GridEdgeKey::new(triangle.vertices[left], triangle.vertices[right]);
                    let interval = *intervals
                        .get(&key)
                        .ok_or(crate::interpolation::InterpolationError::MissingEdgePair)?;
                    let (start, end) = if key.start == triangle.vertices[left] {
                        (left, right)
                    } else {
                        (right, left)
                    };
                    directed.push(DirectedAlphaInterval::new(start, end, interval)?);
                }
                models.push(CubicAlphaTriangle::new(
                    values,
                    directed.try_into().expect("three local edge pairs"),
                    options.extrapolation,
                )?);
            }
            Ok(Self {
                field,
                triangles,
                models,
                diagnostics,
            })
        }
    }

    /// Construction diagnostics for the shared edge model.
    pub fn diagnostics(&self) -> &CubicBuildDiagnostics {
        &self.diagnostics
    }
    /// Elementary triangles in the canonical regular-grid ordering.
    pub fn elementary_triangles(&self) -> &[GridTriangle] {
        &self.triangles
    }
    /// Semantic compositions at one elementary triangle's vertices.
    pub fn triangle_vertices(&self, index: usize) -> Result<[[f64; 3]; 3], FieldError> {
        let [v0, v1, v2] = self
            .triangles
            .get(index)
            .ok_or(FieldError::InvalidVertexIndex {
                index,
                vertex_count: self.triangles.len(),
            })?
            .vertices;
        Ok([
            self.field.composition(v0)?,
            self.field.composition(v1)?,
            self.field.composition(v2)?,
        ])
    }
    /// Evaluate the selected local field at barycentric coordinates.
    pub fn value_in_triangle(&self, index: usize, barycentric: [f64; 3]) -> Option<f64> {
        self.models.get(index).map(|model| model.value(barycentric))
    }
    /// Evaluate the selected local field's reduced barycentric gradient.
    pub fn gradient_in_triangle(&self, index: usize, u: f64, v: f64) -> Option<[f64; 2]> {
        self.models
            .get(index)
            .map(|model| model.gradient_reduced(u, v))
    }
}

#[cfg(feature = "cubic-alpha")]
fn build_edge_intervals(
    field: &RegularTernaryScalarField,
    options: CubicAlphaBuildOptions,
    diagnostics: &mut CubicBuildDiagnostics,
) -> Result<BTreeMap<GridEdgeKey, AlphaInterval>, FieldError> {
    let n = field.subdivisions();
    let mut lines: Vec<Vec<GridVertexId>> = Vec::new();
    for fixed in 0..=n {
        lines.push(
            (0..=n - fixed)
                .map(|i| {
                    field.vertex_id(LatticeCoordinate {
                        i,
                        j: n - fixed - i,
                        k: fixed,
                    })
                })
                .collect::<Result<_, _>>()?,
        );
        lines.push(
            (0..=n - fixed)
                .map(|i| {
                    field.vertex_id(LatticeCoordinate {
                        i,
                        j: fixed,
                        k: n - fixed - i,
                    })
                })
                .collect::<Result<_, _>>()?,
        );
        lines.push(
            (0..=n - fixed)
                .map(|j| {
                    field.vertex_id(LatticeCoordinate {
                        i: fixed,
                        j,
                        k: n - fixed - j,
                    })
                })
                .collect::<Result<_, _>>()?,
        );
    }
    let mut result = BTreeMap::new();
    for line in lines {
        if line.len() < 2 {
            continue;
        }
        let values = line
            .iter()
            .map(|id| field.value(*id))
            .collect::<Result<Vec<_>, _>>()?;
        for interval_index in 0..line.len() - 1 {
            let mut alpha = if line.len() < 3 {
                match options.boundary_policy {
                    CubicBoundaryPolicy::LinearFallback => {
                        diagnostics.linear_fallback_edges += 1;
                        AlphaInterval::default()
                    }
                    CubicBoundaryPolicy::Error => {
                        return Err(FieldError::InsufficientStencil {
                            samples: line.len(),
                        });
                    }
                }
            } else {
                diagnostics.cubic_edges += 1;
                alpha_for_interval(options.method, &values, interval_index)
            };
            let key = GridEdgeKey::new(line[interval_index], line[interval_index + 1]);
            if key.start != line[interval_index] {
                alpha = alpha.reversed();
            }
            result.insert(key, alpha);
        }
    }
    debug_assert_eq!(result.len(), field.edge_count()?);
    Ok(result)
}

#[cfg(feature = "cubic-alpha")]
fn alpha_for_interval(method: CubicAlphaMethod, values: &[f64], index: usize) -> AlphaInterval {
    use spline1d::{cubic_single_left_alpha, cubic_single_middle_alpha, cubic_single_right_alpha};
    let kind = method_kind(method);
    let alpha = if index == 0 {
        cubic_single_left_alpha(kind, 0.0, values[0], 1.0, values[1], 2.0, values[2])
    } else if index + 1 == values.len() - 1 {
        let base = index - 1;
        cubic_single_right_alpha(
            kind,
            base as f64,
            values[base],
            index as f64,
            values[index],
            (index + 1) as f64,
            values[index + 1],
        )
    } else {
        cubic_single_middle_alpha(
            kind,
            (index - 1) as f64,
            values[index - 1],
            index as f64,
            values[index],
            (index + 1) as f64,
            values[index + 1],
            (index + 2) as f64,
            values[index + 2],
        )
    };
    AlphaInterval::new(alpha[0], alpha[1])
}

#[cfg(feature = "cubic-alpha")]
fn method_kind(method: CubicAlphaMethod) -> spline1d::InterpolationType<f64> {
    match method {
        CubicAlphaMethod::Akima => spline1d::InterpolationType::AKIMA,
        CubicAlphaMethod::Makima => spline1d::InterpolationType::MAKIMA,
        CubicAlphaMethod::Pchip => spline1d::InterpolationType::PCHIP,
        CubicAlphaMethod::Steffen => spline1d::InterpolationType::STEFFEN,
    }
}

#[cfg(all(test, feature = "cubic-alpha"))]
mod tests {
    use super::*;
    #[test]
    fn shared_edges_are_built_once_with_expected_fallbacks() {
        let field = RegularTernaryScalarField::new(3, (0..10).map(|i| i as f64).collect()).unwrap();
        let model = CubicGridField::new(&field, CubicAlphaBuildOptions::default()).unwrap();
        assert_eq!(model.elementary_triangles().len(), 9);
        assert_eq!(
            model.diagnostics().cubic_edges + model.diagnostics().linear_fallback_edges,
            field.edge_count().unwrap()
        );
        assert_eq!(model.diagnostics().linear_fallback_edges, 3);
    }
}
