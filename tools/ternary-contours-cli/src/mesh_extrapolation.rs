//! TCT-facing preview and transactional materialization of regular-mesh EX values.
//!
//! The numerical continuation itself lives in `ternary-contours`. This module
//! only maps typed TCT cells to its `Option<f64>` input and persists reviewed
//! provenance back into the dataset.

use ternary_contours::{
    RegularMeshExtrapolatedValue, RegularMeshExtrapolationDiagnostics,
    RegularMeshExtrapolationOptions, extrapolate_regular_mesh,
};

use crate::{
    ExtrapolatedValueMetadata, GridType, RegularTabulatedGrid, TabulatedField, TabulatedGrid,
    TabulatedTernaryDataset, TabulatedValue, TabulatedValueState,
};

/// A named phase/property field selected for mesh extrapolation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MeshExtrapolationField {
    /// Phase declaration name, matched exactly.
    pub phase: String,
    /// Property declaration name, matched exactly.
    pub property: String,
}

impl MeshExtrapolationField {
    /// Parse the unambiguous CLI spelling `Phase.Property`.
    pub fn parse(value: &str) -> Result<Self, MeshExtrapolationError> {
        let (phase, property) = value.rsplit_once('.').ok_or_else(|| {
            MeshExtrapolationError::InvalidRequest(format!(
                "field `{value}` must be written as Phase.Property"
            ))
        })?;
        if phase.trim().is_empty() || property.trim().is_empty() {
            return Err(MeshExtrapolationError::InvalidRequest(format!(
                "field `{value}` must contain both a phase and property"
            )));
        }
        Ok(Self {
            phase: phase.trim().to_owned(),
            property: property.trim().to_owned(),
        })
    }
}

/// Immutable input to a regular-mesh extrapolation preview.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshExtrapolationRequest {
    /// Grid name selected by the user.
    pub grid: String,
    /// Explicit fields. Empty is permitted only with [`Self::all_fields`].
    pub fields: Vec<MeshExtrapolationField>,
    /// Select every field in the selected grid.
    pub all_fields: bool,
    /// Numerical safety policy.
    pub options: RegularMeshExtrapolationOptions,
}

/// Materialized values proposed for one TCT field.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshExtrapolationFieldPreview {
    /// Field identity as stored in the grid.
    pub phase_id: ternary_contours::StablePhaseId,
    pub property: String,
    /// EX values in canonical regular-grid row order.
    pub values: Vec<RegularMeshExtrapolatedValue>,
    /// Core diagnostics, kept visible even when no values were created.
    pub diagnostics: RegularMeshExtrapolationDiagnostics,
}

/// A reviewed, stale-safe materialization plan.
#[derive(Clone, Debug, PartialEq)]
pub struct MeshExtrapolationPreview {
    /// Grid identity in the dataset snapshot.
    pub grid_index: usize,
    /// Selected grid name, retained for clear error messages.
    pub grid_name: String,
    /// Fingerprint of every selected source field at preview time.
    pub source_fingerprint: u64,
    /// Method and guards used to construct this exact preview.
    pub options: RegularMeshExtrapolationOptions,
    /// Per-field values to materialize.
    pub fields: Vec<MeshExtrapolationFieldPreview>,
}

/// Aggregate result of applying one reviewed preview.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MeshExtrapolationApplySummary {
    /// Number of fields changed.
    pub fields_changed: usize,
    /// Number of EX cells created.
    pub values_created: usize,
    /// Largest created synchronous layer.
    pub maximum_layer: u16,
}

/// Typed request and transaction failures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MeshExtrapolationError {
    /// The target grid does not exist.
    UnknownGrid { grid: String },
    /// Auto extrapolation intentionally has no irregular-grid implementation.
    UnsupportedGridKind { grid_kind: GridType },
    /// A named field was absent from the selected grid.
    UnknownField { grid: String, field: String },
    /// The request did not name any fields.
    NoFieldsSelected,
    /// Preview source data has changed and must be reviewed again.
    StalePreview { grid: String },
    /// An input request is malformed.
    InvalidRequest(String),
    /// Numerical core rejected regular-grid source data or options.
    Core(String),
    /// The resulting TCT document was structurally invalid.
    InvalidDocument(String),
}

impl core::fmt::Display for MeshExtrapolationError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownGrid { grid } => write!(formatter, "grid `{grid}` does not exist"),
            Self::UnsupportedGridKind { grid_kind } => write!(
                formatter,
                "automatic mesh extrapolation is currently available for regular grids only (selected {grid_kind} grid)"
            ),
            Self::UnknownField { grid, field } => {
                write!(formatter, "grid `{grid}` has no field `{field}`")
            }
            Self::NoFieldsSelected => {
                formatter.write_str("select at least one grid field or use --all-fields")
            }
            Self::StalePreview { grid } => write!(
                formatter,
                "mesh extrapolation preview for grid `{grid}` is stale; create a new preview before materializing"
            ),
            Self::InvalidRequest(message)
            | Self::Core(message)
            | Self::InvalidDocument(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for MeshExtrapolationError {}

/// Preview deterministic extrapolation without mutating the dataset.
pub fn extrapolate_regular_grid_fields(
    dataset: &TabulatedTernaryDataset,
    request: &MeshExtrapolationRequest,
) -> Result<MeshExtrapolationPreview, MeshExtrapolationError> {
    let (grid_index, grid) = dataset
        .grids
        .iter()
        .enumerate()
        .find(|(_, grid)| grid.name() == request.grid)
        .ok_or_else(|| MeshExtrapolationError::UnknownGrid {
            grid: request.grid.clone(),
        })?;
    let TabulatedGrid::Regular(grid) = grid else {
        return Err(MeshExtrapolationError::UnsupportedGridKind {
            grid_kind: GridType::Irregular,
        });
    };
    let field_indices = selected_field_indices(dataset, grid, request)?;
    let mut fields = Vec::with_capacity(field_indices.len());
    for field_index in field_indices {
        let field = &grid.fields[field_index];
        let values = field
            .values
            .iter()
            .map(TabulatedValue::defined_value)
            .collect::<Vec<_>>();
        let eligible = field
            .values
            .iter()
            .map(|value| matches!(value.state, TabulatedValueState::Missing))
            .collect::<Vec<_>>();
        let core = extrapolate_regular_mesh(
            ternary_contours::RegularTernaryGrid::new(grid.subdivisions)
                .map_err(|error| MeshExtrapolationError::Core(error.to_string()))?,
            &values,
            &eligible,
            request.options,
        )
        .map_err(|error| MeshExtrapolationError::Core(error.to_string()))?;
        fields.push(MeshExtrapolationFieldPreview {
            phase_id: field.phase_id,
            property: field.property.clone(),
            values: core.values,
            diagnostics: core.diagnostics,
        });
    }
    Ok(MeshExtrapolationPreview {
        grid_index,
        grid_name: grid.name.clone(),
        source_fingerprint: preview_fingerprint(dataset, grid_index, &fields),
        options: request.options,
        fields,
    })
}

/// Apply exactly the reviewed values after verifying the source fingerprint.
pub fn apply_mesh_extrapolation(
    dataset: &mut TabulatedTernaryDataset,
    preview: MeshExtrapolationPreview,
) -> Result<MeshExtrapolationApplySummary, MeshExtrapolationError> {
    if dataset
        .grids
        .get(preview.grid_index)
        .map(TabulatedGrid::name)
        != Some(preview.grid_name.as_str())
    {
        return Err(MeshExtrapolationError::StalePreview {
            grid: preview.grid_name,
        });
    }
    if preview_fingerprint(dataset, preview.grid_index, &preview.fields)
        != preview.source_fingerprint
    {
        return Err(MeshExtrapolationError::StalePreview {
            grid: preview.grid_name,
        });
    }
    let TabulatedGrid::Regular(grid) = &mut dataset.grids[preview.grid_index] else {
        return Err(MeshExtrapolationError::UnsupportedGridKind {
            grid_kind: GridType::Irregular,
        });
    };
    let mut summary = MeshExtrapolationApplySummary::default();
    for field_preview in preview.fields {
        let field = grid
            .fields
            .iter_mut()
            .find(|field| {
                field.phase_id == field_preview.phase_id && field.property == field_preview.property
            })
            .ok_or_else(|| MeshExtrapolationError::StalePreview {
                grid: grid.name.clone(),
            })?;
        if field_preview.values.is_empty() {
            continue;
        }
        for value in field_preview.values {
            let target = field.values.get_mut(value.vertex_index).ok_or_else(|| {
                MeshExtrapolationError::InvalidDocument(format!(
                    "grid `{}` field `{}.{}` has no canonical row {}",
                    grid.name,
                    field.phase_id.0,
                    field.property,
                    value.vertex_index + 1
                ))
            })?;
            if !matches!(target.state, TabulatedValueState::Missing) {
                return Err(MeshExtrapolationError::StalePreview {
                    grid: grid.name.clone(),
                });
            }
            *target = TabulatedValue::extrapolated(
                value.value,
                ExtrapolatedValueMetadata {
                    layer: value.layer,
                    method: value.method,
                    support_count: u16::try_from(value.directional_support_count).map_err(
                        |_| {
                            MeshExtrapolationError::Core(
                                "directional support count exceeds TCT provenance capacity".into(),
                            )
                        },
                    )?,
                    spread: value.spread,
                },
            )
            .map_err(MeshExtrapolationError::InvalidDocument)?;
            summary.values_created += 1;
            summary.maximum_layer = summary.maximum_layer.max(value.layer);
        }
        summary.fields_changed += 1;
    }
    dataset
        .validate_document_structure()
        .map_err(MeshExtrapolationError::InvalidDocument)?;
    Ok(summary)
}

fn selected_field_indices(
    dataset: &TabulatedTernaryDataset,
    grid: &RegularTabulatedGrid,
    request: &MeshExtrapolationRequest,
) -> Result<Vec<usize>, MeshExtrapolationError> {
    if request.all_fields {
        return (!grid.fields.is_empty())
            .then(|| (0..grid.fields.len()).collect())
            .ok_or(MeshExtrapolationError::NoFieldsSelected);
    }
    if request.fields.is_empty() {
        return Err(MeshExtrapolationError::NoFieldsSelected);
    }
    request
        .fields
        .iter()
        .map(|requested| {
            let phase = dataset
                .phases
                .iter()
                .find(|phase| phase.name == requested.phase)
                .ok_or_else(|| MeshExtrapolationError::UnknownField {
                    grid: grid.name.clone(),
                    field: format!("{}.{}", requested.phase, requested.property),
                })?;
            grid.fields
                .iter()
                .position(|field| {
                    field.phase_id == phase.id && field.property == requested.property
                })
                .ok_or_else(|| MeshExtrapolationError::UnknownField {
                    grid: grid.name.clone(),
                    field: format!("{}.{}", requested.phase, requested.property),
                })
        })
        .collect()
}

fn preview_fingerprint(
    dataset: &TabulatedTernaryDataset,
    grid_index: usize,
    previews: &[MeshExtrapolationFieldPreview],
) -> u64 {
    // Stable FNV-1a over the selected original fields. This is a stale-preview
    // guard, not a security primitive.
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let Some(grid) = dataset.grids.get(grid_index) else {
        return 0;
    };
    for preview in previews {
        hash_bytes(&mut hash, &preview.phase_id.0.to_le_bytes());
        hash_bytes(&mut hash, preview.property.as_bytes());
        if let Some(field) = grid
            .fields()
            .iter()
            .find(|field| field.phase_id == preview.phase_id && field.property == preview.property)
        {
            hash_field(&mut hash, field);
        }
    }
    hash
}

fn hash_field(hash: &mut u64, field: &TabulatedField) {
    for cell in &field.values {
        hash_bytes(hash, &[cell.state as u8]);
        if let Some(value) = cell.value {
            hash_bytes(hash, &value.to_bits().to_le_bytes());
        }
        if let Some(metadata) = &cell.extrapolation {
            hash_bytes(hash, &metadata.layer.to_le_bytes());
            hash_bytes(hash, &[metadata.method as u8]);
            hash_bytes(hash, &metadata.support_count.to_le_bytes());
            hash_bytes(hash, &metadata.spread.to_bits().to_le_bytes());
        }
        if let Some(note) = &cell.note {
            hash_bytes(hash, note.as_bytes());
        }
        hash_bytes(hash, &[0xff]);
    }
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_regular_dataset;

    #[test]
    fn preview_is_regular_only_and_apply_materializes_ex_cells() {
        let mut dataset = default_regular_dataset();
        let TabulatedGrid::Regular(grid) = &mut dataset.grids[0] else {
            unreachable!()
        };
        for (row, value) in grid.fields[0].values.iter_mut().enumerate() {
            *value = TabulatedValue::calculated(1000.0 + row as f64).unwrap();
        }
        grid.fields[0].values[0] = TabulatedValue::missing();
        let request = MeshExtrapolationRequest {
            grid: "regular".into(),
            fields: vec![MeshExtrapolationField {
                phase: "Phase1".into(),
                property: "T".into(),
            }],
            all_fields: false,
            options: RegularMeshExtrapolationOptions::default(),
        };
        let preview = extrapolate_regular_grid_fields(&dataset, &request).unwrap();
        assert!(!preview.fields[0].values.is_empty());
        let applied = apply_mesh_extrapolation(&mut dataset, preview).unwrap();
        assert!(applied.values_created >= 1);
        assert_eq!(
            dataset.grids[0].fields()[0].values[0].state,
            TabulatedValueState::Extrapolated
        );
    }
}
