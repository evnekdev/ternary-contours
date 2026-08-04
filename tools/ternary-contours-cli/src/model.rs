use std::path::PathBuf;

use ternary_contours::{RegularTernaryGrid, StablePhaseId};

/// TCT format version declared by the file header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FormatVersion {
    pub major: u16,
    pub minor: u16,
}

impl core::fmt::Display for FormatVersion {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(formatter, "{}.{}", self.major, self.minor)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentDefinition {
    pub name: String,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhaseDefinition {
    pub name: String,
    pub id: StablePhaseId,
    pub line: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PropertyDefinition {
    pub name: String,
    pub required: bool,
    pub unit: String,
    pub line: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GridType {
    Regular,
    Irregular,
}

impl core::fmt::Display for GridType {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Regular => "regular",
            Self::Irregular => "irregular",
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowOrder {
    Canonical,
    Compositions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompositionColumns {
    None,
    Guidance,
    Authoritative,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceRange {
    pub first_line: usize,
    pub last_line: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TabulatedField {
    pub phase_id: StablePhaseId,
    pub property: String,
    pub column_name: String,
    pub values: Vec<Option<f64>>,
    pub row_lines: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RegularTabulatedGrid {
    pub name: String,
    pub source: SourceRange,
    pub subdivisions: usize,
    pub order: RowOrder,
    pub composition_columns: CompositionColumns,
    pub compositions: Vec<[f64; 3]>,
    pub fields: Vec<TabulatedField>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IrregularTabulatedGrid {
    pub name: String,
    pub source: SourceRange,
    pub compositions: Vec<[f64; 3]>,
    pub fields: Vec<TabulatedField>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TabulatedGrid {
    Regular(RegularTabulatedGrid),
    Irregular(IrregularTabulatedGrid),
}

impl TabulatedGrid {
    pub fn name(&self) -> &str {
        match self {
            Self::Regular(grid) => &grid.name,
            Self::Irregular(grid) => &grid.name,
        }
    }

    pub const fn grid_type(&self) -> GridType {
        match self {
            Self::Regular(_) => GridType::Regular,
            Self::Irregular(_) => GridType::Irregular,
        }
    }

    pub fn compositions(&self) -> &[[f64; 3]] {
        match self {
            Self::Regular(grid) => &grid.compositions,
            Self::Irregular(grid) => &grid.compositions,
        }
    }

    pub fn fields(&self) -> &[TabulatedField] {
        match self {
            Self::Regular(grid) => &grid.fields,
            Self::Irregular(grid) => &grid.fields,
        }
    }
}

/// Parser output with no renderer, file-format, or numerical-library ownership.
#[derive(Clone, Debug, PartialEq)]
pub struct TabulatedTernaryDataset {
    pub source_path: Option<PathBuf>,
    pub version: FormatVersion,
    pub title: Option<String>,
    pub composition_units: Option<String>,
    pub missing_tokens: Vec<String>,
    pub components: [ComponentDefinition; 3],
    pub phases: Vec<PhaseDefinition>,
    pub properties: Vec<PropertyDefinition>,
    pub grids: Vec<TabulatedGrid>,
    pub warnings: Vec<String>,
}

/// Construct the editable dataset shown when the viewer starts without a file.
///
/// The required T property is declared for every phase, but its values are
/// intentionally undefined until the user pastes or enters them in the Data
/// tab. This is a structurally valid dataset; projection calculation will
/// report a useful missing-data error until the fields are populated.
pub fn default_regular_dataset() -> TabulatedTernaryDataset {
    let subdivisions = 10;
    let compositions = RegularTernaryGrid::new(subdivisions)
        .expect("the built-in default subdivision count is valid")
        .compositions()
        .collect::<Vec<_>>();
    let row_count = compositions.len();
    let fields = [
        (StablePhaseId(1), "Phase1"),
        (StablePhaseId(2), "Phase2"),
        (StablePhaseId(3), "Phase3"),
    ]
    .into_iter()
    .map(|(phase_id, phase_name)| TabulatedField {
        phase_id,
        property: "T".into(),
        column_name: format!("{}.T", phase_name),
        values: vec![None; row_count],
        row_lines: vec![0; row_count],
    })
    .collect();

    TabulatedTernaryDataset {
        source_path: None,
        version: FormatVersion { major: 1, minor: 0 },
        title: Some("Untitled ternary system".into()),
        composition_units: None,
        missing_tokens: vec!["NA".into()],
        components: [
            ComponentDefinition {
                name: "A".into(),
                line: 0,
            },
            ComponentDefinition {
                name: "B".into(),
                line: 0,
            },
            ComponentDefinition {
                name: "C".into(),
                line: 0,
            },
        ],
        phases: vec![
            PhaseDefinition {
                name: "Phase1".into(),
                id: StablePhaseId(1),
                line: 0,
            },
            PhaseDefinition {
                name: "Phase2".into(),
                id: StablePhaseId(2),
                line: 0,
            },
            PhaseDefinition {
                name: "Phase3".into(),
                id: StablePhaseId(3),
                line: 0,
            },
        ],
        properties: vec![PropertyDefinition {
            name: "T".into(),
            required: true,
            unit: "K".into(),
            line: 0,
        }],
        grids: vec![TabulatedGrid::Regular(RegularTabulatedGrid {
            name: "regular".into(),
            source: SourceRange {
                first_line: 0,
                last_line: 0,
            },
            subdivisions,
            order: RowOrder::Canonical,
            composition_columns: CompositionColumns::None,
            compositions,
            fields,
        })],
        warnings: Vec::new(),
    }
}
impl TabulatedTernaryDataset {
    pub fn phase_by_name(&self, name: &str) -> Option<&PhaseDefinition> {
        self.phases.iter().find(|phase| phase.name == name)
    }

    pub fn property(&self, name: &str) -> Option<&PropertyDefinition> {
        self.properties
            .iter()
            .find(|property| property.name == name)
    }

    /// Validate declarations and grid references without requiring scalar values.
    pub fn validate_structure(&self) -> Result<(), String> {
        if self
            .components
            .iter()
            .any(|component| component.name.trim().is_empty())
            || self
                .components
                .iter()
                .enumerate()
                .any(|(index, component)| {
                    self.components[..index]
                        .iter()
                        .any(|previous| previous.name == component.name)
                })
        {
            return Err("component names must be non-empty and unique".into());
        }
        if self.phases.is_empty() {
            return Err("at least one phase is required".into());
        }
        if self.phases.iter().any(|phase| phase.name.trim().is_empty())
            || self.phases.iter().enumerate().any(|(index, phase)| {
                self.phases[..index]
                    .iter()
                    .any(|previous| previous.name == phase.name || previous.id == phase.id)
            })
            || self.phases.iter().any(|phase| phase.id.0 == 0)
        {
            return Err("phase names and positive IDs must be unique".into());
        }
        if self
            .properties
            .iter()
            .any(|property| property.name.trim().is_empty())
            || self.properties.iter().enumerate().any(|(index, property)| {
                self.properties[..index]
                    .iter()
                    .any(|previous| previous.name == property.name)
            })
        {
            return Err("property names must be non-empty and unique".into());
        }
        let Some(temperature) = self.properties.iter().find(|property| property.name == "T") else {
            return Err("required property T is missing".into());
        };
        if !temperature.required {
            return Err("property T must remain required".into());
        }
        if self.grids.is_empty() {
            return Err("at least one grid is required".into());
        }
        for grid in &self.grids {
            let expected = match grid {
                TabulatedGrid::Regular(value) => {
                    if value.subdivisions == 0 {
                        return Err(format!(
                            "grid {} must have a positive subdivision count",
                            grid.name()
                        ));
                    }
                    let expected = value
                        .subdivisions
                        .checked_add(1)
                        .and_then(|left| {
                            value
                                .subdivisions
                                .checked_add(2)
                                .and_then(|right| left.checked_mul(right))
                        })
                        .map(|value| value / 2)
                        .ok_or_else(|| {
                            format!("grid {} subdivision count overflows", grid.name())
                        })?;
                    if value.compositions.len() != expected {
                        return Err(format!(
                            "grid {} has {} compositions; expected {}",
                            grid.name(),
                            value.compositions.len(),
                            expected
                        ));
                    }
                    Some(expected)
                }
                TabulatedGrid::Irregular(_) => None,
            };
            for field in grid.fields() {
                if !self
                    .phases
                    .iter()
                    .any(|candidate| candidate.id == field.phase_id)
                    || !self
                        .properties
                        .iter()
                        .any(|candidate| candidate.name == field.property)
                {
                    return Err(format!(
                        "grid {} references unknown field {}.{}",
                        grid.name(),
                        field.phase_id.0,
                        field.property
                    ));
                }
                if expected.is_some_and(|count| field.values.len() != count) {
                    return Err(format!(
                        "grid {} field {}.{} has {} values; expected {}",
                        grid.name(),
                        field.phase_id.0,
                        field.property,
                        field.values.len(),
                        expected.unwrap_or_default()
                    ));
                }
            }
        }
        for phase in &self.phases {
            if !self.grids.iter().any(|grid| {
                grid.fields()
                    .iter()
                    .any(|field| field.phase_id == phase.id && field.property == "T")
            }) {
                return Err(format!("phase {} has no required T field", phase.name));
            }
        }
        Ok(())
    }
    pub fn field_count(&self) -> usize {
        self.grids.iter().map(|grid| grid.fields().len()).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_regular_dataset_is_editable_and_structurally_complete() {
        let dataset = default_regular_dataset();
        assert_eq!(dataset.title.as_deref(), Some("Untitled ternary system"));
        assert_eq!(
            dataset
                .components
                .iter()
                .map(|component| component.name.as_str())
                .collect::<Vec<_>>(),
            ["A", "B", "C"]
        );
        assert_eq!(
            dataset
                .phases
                .iter()
                .map(|phase| (phase.name.as_str(), phase.id.0))
                .collect::<Vec<_>>(),
            [("Phase1", 1), ("Phase2", 2), ("Phase3", 3)]
        );
        assert_eq!(
            dataset.properties,
            vec![PropertyDefinition {
                name: "T".into(),
                required: true,
                unit: "K".into(),
                line: 0,
            }]
        );
        let TabulatedGrid::Regular(grid) = &dataset.grids[0] else {
            panic!("default dataset must use a regular grid");
        };
        assert_eq!(grid.subdivisions, 10);
        assert_eq!(grid.compositions.len(), 66);
        assert_eq!(
            grid.fields
                .iter()
                .map(|field| field.column_name.as_str())
                .collect::<Vec<_>>(),
            ["Phase1.T", "Phase2.T", "Phase3.T"]
        );
        assert!(
            grid.fields
                .iter()
                .all(|field| field.values.iter().all(Option::is_none))
        );
        let text = crate::serialize_tct(&dataset, &crate::TctSerializeOptions::default()).unwrap();
        let round_trip = crate::parse_str(&text).unwrap();
        assert_eq!(round_trip.title, dataset.title);
        assert_eq!(
            round_trip.grids[0].compositions(),
            dataset.grids[0].compositions()
        );
    }
}
