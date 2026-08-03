use std::path::PathBuf;

use ternary_contours::StablePhaseId;

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

impl TabulatedTernaryDataset {
    pub fn phase_by_name(&self, name: &str) -> Option<&PhaseDefinition> {
        self.phases.iter().find(|phase| phase.name == name)
    }

    pub fn property(&self, name: &str) -> Option<&PropertyDefinition> {
        self.properties
            .iter()
            .find(|property| property.name == name)
    }

    pub fn field_count(&self) -> usize {
        self.grids.iter().map(|grid| grid.fields().len()).sum()
    }
}
