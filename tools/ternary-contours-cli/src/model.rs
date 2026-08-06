use std::path::PathBuf;

use ternary_contours::{CubicAlphaMethod, RegularTernaryGrid, StablePhaseId};

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

/// Classification of one tabulated scalar entry.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum TabulatedValueState {
    /// A finite scalar supplied by the source calculation.
    Calculated,
    /// A finite scalar estimated from a regular mesh with persistent provenance.
    Extrapolated,
    /// The calculation exceeded an explicit high-temperature limit.
    CutOff,
    /// The point has not yet been calculated or classified.
    #[default]
    Missing,
}

impl TabulatedValueState {
    pub const fn token(self) -> &'static str {
        match self {
            Self::Calculated => "OK",
            Self::Extrapolated => "EX",
            Self::CutOff => "CO",
            Self::Missing => "NA",
        }
    }
}

/// Machine-readable provenance for an [`TabulatedValueState::Extrapolated`] cell.
#[derive(Clone, Debug, PartialEq)]
pub struct ExtrapolatedValueMetadata {
    pub layer: u16,
    pub method: CubicAlphaMethod,
    pub support_count: u16,
    pub spread: f64,
}

/// One typed scalar cell. Only calculated and extrapolated cells carry values.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TabulatedValue {
    pub state: TabulatedValueState,
    pub value: Option<f64>,
    pub extrapolation: Option<ExtrapolatedValueMetadata>,
    pub note: Option<String>,
}

impl TabulatedValue {
    pub fn calculated(value: f64) -> Result<Self, String> {
        value
            .is_finite()
            .then_some(Self {
                state: TabulatedValueState::Calculated,
                value: Some(value),
                extrapolation: None,
                note: None,
            })
            .ok_or_else(|| "calculated values must be finite".into())
    }
    pub const fn missing() -> Self {
        Self {
            state: TabulatedValueState::Missing,
            value: None,
            extrapolation: None,
            note: None,
        }
    }
    pub const fn cut_off() -> Self {
        Self {
            state: TabulatedValueState::CutOff,
            value: None,
            extrapolation: None,
            note: None,
        }
    }
    pub fn extrapolated(
        value: f64,
        extrapolation: ExtrapolatedValueMetadata,
    ) -> Result<Self, String> {
        let result = Self {
            state: TabulatedValueState::Extrapolated,
            value: Some(value),
            extrapolation: Some(extrapolation),
            note: None,
        };
        result.validate()?;
        Ok(result)
    }
    /// Explicit source-calculation scalar, excluding EX values.
    pub const fn calculated_value(&self) -> Option<f64> {
        match self {
            Self {
                state: TabulatedValueState::Calculated,
                value: Some(value),
                ..
            } => Some(*value),
            _ => None,
        }
    }
    /// Finite scalar accepted by source interpolation.
    pub const fn defined_value(&self) -> Option<f64> {
        match self {
            Self {
                state: TabulatedValueState::Calculated | TabulatedValueState::Extrapolated,
                value: Some(value),
                ..
            } => Some(*value),
            _ => None,
        }
    }
    pub const fn is_calculated(&self) -> bool {
        matches!(self.state, TabulatedValueState::Calculated)
    }
    pub const fn is_defined(&self) -> bool {
        matches!(
            self.state,
            TabulatedValueState::Calculated | TabulatedValueState::Extrapolated
        )
    }
    /// Clears persistent extrapolation provenance when an input field is edited.
    ///
    /// Extrapolated cells are derived from their field neighbours. A coarse
    /// field-level invalidation rule deliberately turns every `EX` cell in a
    /// manually edited field back into typed `NA`.
    pub fn clear_if_extrapolated(&mut self) -> bool {
        if self.state == TabulatedValueState::Extrapolated {
            *self = Self::missing();
            true
        } else {
            false
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self
            .note
            .as_deref()
            .is_some_and(|note| note.contains(['\t', '\n', '\r']))
        {
            return Err("classified-value notes cannot contain tabs or line breaks".into());
        }
        match (
            self.state,
            self.value,
            self.extrapolation.as_ref(),
            self.note.as_deref(),
        ) {
            (TabulatedValueState::Calculated, Some(value), None, None) if value.is_finite() => {
                Ok(())
            }
            (TabulatedValueState::Calculated, _, _, _) => {
                Err("calculated values require one finite scalar and no provenance or note".into())
            }
            (TabulatedValueState::Extrapolated, Some(value), Some(metadata), None)
                if value.is_finite()
                    && metadata.layer >= 1
                    && metadata.support_count >= 1
                    && metadata.spread.is_finite()
                    && metadata.spread >= 0.0 =>
            {
                Ok(())
            }
            (TabulatedValueState::Extrapolated, _, _, _) => {
                Err("extrapolated values require one finite scalar and valid provenance".into())
            }
            (TabulatedValueState::Missing | TabulatedValueState::CutOff, None, None, _) => Ok(()),
            (state, Some(_), _, _) => Err(format!(
                "{} values must not carry an active calculated scalar",
                state.token()
            )),
            (state, None, Some(_), _) => Err(format!(
                "{} values must not carry extrapolation provenance",
                state.token()
            )),
        }
    }
    pub fn token_with_format(
        &self,
        numeric: impl FnOnce(f64) -> String,
        missing_token: &str,
    ) -> String {
        match self.state {
            TabulatedValueState::Calculated => self
                .value
                .filter(|value| value.is_finite())
                .map(numeric)
                .unwrap_or_else(|| missing_token.to_owned()),
            TabulatedValueState::Extrapolated => self
                .value
                .filter(|value| value.is_finite())
                .zip(self.extrapolation.as_ref())
                .map(|(value, metadata)| {
                    format!(
                        "EX[{},{},{},{:.17}]={}",
                        metadata.layer,
                        cubic_method_token(metadata.method),
                        metadata.support_count,
                        metadata.spread,
                        numeric(value)
                    )
                })
                .unwrap_or_else(|| missing_token.to_owned()),
            TabulatedValueState::Missing => state_token("NA", self.note.as_deref(), missing_token),
            TabulatedValueState::CutOff => state_token("CO", self.note.as_deref(), missing_token),
        }
    }
}

fn state_token(state: &str, note: Option<&str>, missing_token: &str) -> String {
    match (state, note.filter(|note| !note.trim().is_empty())) {
        ("NA", None) => missing_token.to_owned(),
        (_, Some(note)) => format!("{state}:{}", note.trim()),
        _ => state.to_owned(),
    }
}

/// Parse TCT and TSV scalar cells. Legacy `NE` is normalized to `NA`.
pub fn parse_tabulated_value_token(
    token: &str,
    missing_tokens: &[String],
    blank_is_missing: bool,
) -> Result<TabulatedValue, String> {
    let token = token.trim();
    if token.is_empty() {
        return blank_is_missing
            .then_some(TabulatedValue::missing())
            .ok_or_else(|| {
                "blank cells are not zero; enable blank-as-missing to accept them".into()
            });
    }
    if token.len() >= 5
        && token
            .get(..2)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("EX"))
    {
        return parse_extrapolated_value(token);
    }
    let state = |prefix: &str, state: TabulatedValueState| {
        let exact = token.eq_ignore_ascii_case(prefix);
        let annotated = token
            .get(..prefix.len())
            .filter(|head| head.eq_ignore_ascii_case(prefix))
            .and_then(|_| token.get(prefix.len()..))
            .filter(|suffix| suffix.starts_with(':'));
        (exact || annotated.is_some()).then(|| TabulatedValue {
            state,
            value: None,
            extrapolation: None,
            note: annotated
                .map(|suffix| suffix[1..].trim())
                .filter(|note| !note.is_empty())
                .map(ToOwned::to_owned),
        })
    };
    if let Some(value) = state("NE", TabulatedValueState::Missing)
        .or_else(|| state("CO", TabulatedValueState::CutOff))
        .or_else(|| state("NA", TabulatedValueState::Missing))
    {
        return Ok(value);
    }
    if missing_tokens.iter().any(|missing| missing == token) {
        return Ok(TabulatedValue::missing());
    }
    token
        .parse::<f64>()
        .map_err(|_| "invalid numeric or classified-state token".to_owned())
        .and_then(TabulatedValue::calculated)
}

fn cubic_method_token(method: CubicAlphaMethod) -> &'static str {
    match method {
        CubicAlphaMethod::Akima => "akima",
        CubicAlphaMethod::Makima => "makima",
        CubicAlphaMethod::Pchip => "pchip",
        CubicAlphaMethod::Steffen => "steffen",
        _ => "unknown",
    }
}
fn parse_extrapolated_value(token: &str) -> Result<TabulatedValue, String> {
    let (metadata, value) = token
        .split_once("]=")
        .ok_or_else(|| "EX values must use EX[layer,method,support,spread]=value".to_owned())?;
    let metadata = metadata
        .get(..3)
        .filter(|prefix| prefix.eq_ignore_ascii_case("EX["))
        .and_then(|_| metadata.get(3..))
        .ok_or_else(|| "EX values must begin with EX[".to_owned())?;
    let values = metadata.split(',').map(str::trim).collect::<Vec<_>>();
    if values.len() != 4 {
        return Err("EX values require layer, method, support count, and spread".into());
    }
    let layer = values[0]
        .parse::<u16>()
        .ok()
        .filter(|layer| *layer >= 1)
        .ok_or_else(|| "EX layer must be an integer of at least one".to_owned())?;
    let method = if values[1].eq_ignore_ascii_case("akima") {
        CubicAlphaMethod::Akima
    } else if values[1].eq_ignore_ascii_case("makima") {
        CubicAlphaMethod::Makima
    } else if values[1].eq_ignore_ascii_case("pchip") {
        CubicAlphaMethod::Pchip
    } else if values[1].eq_ignore_ascii_case("steffen") {
        CubicAlphaMethod::Steffen
    } else {
        return Err("EX method must be akima, makima, pchip, or steffen".into());
    };
    let support_count = values[2]
        .parse::<u16>()
        .ok()
        .filter(|count| *count >= 1)
        .ok_or_else(|| "EX support count must be an integer of at least one".to_owned())?;
    let spread = values[3]
        .parse::<f64>()
        .ok()
        .filter(|spread| spread.is_finite() && *spread >= 0.0)
        .ok_or_else(|| "EX spread must be finite and nonnegative".to_owned())?;
    let value = value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or_else(|| "EX scalar value must be finite".to_owned())?;
    TabulatedValue::extrapolated(
        value,
        ExtrapolatedValueMetadata {
            layer,
            method,
            support_count,
            spread,
        },
    )
}
#[derive(Clone, Debug, PartialEq)]
pub struct TabulatedField {
    pub phase_id: StablePhaseId,
    pub property: String,
    pub column_name: String,
    pub values: Vec<TabulatedValue>,
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

/// Interactive regular-grid resolution bounds used by project authoring.
///
/// These limits apply to newly created/resized grids. Existing files may still
/// contain larger regular grids and remain valid for loading and inspection.
pub const MIN_REGULAR_GRID_SUBDIVISIONS: usize = 1;
pub const MAX_REGULAR_GRID_SUBDIVISIONS: usize = 50;

/// Return the canonical regular-grid point count using checked arithmetic.
pub fn regular_grid_point_count(subdivisions: usize) -> Option<usize> {
    (subdivisions >= MIN_REGULAR_GRID_SUBDIVISIONS)
        .then_some(subdivisions)
        .and_then(|n| n.checked_add(1))
        .and_then(|left| {
            subdivisions
                .checked_add(2)
                .and_then(|right| left.checked_mul(right))
        })
        .map(|product| product / 2)
}

/// Validate a subdivision count for interactive regular-grid creation.
pub fn validate_new_regular_grid_subdivisions(subdivisions: usize) -> Result<(), String> {
    if subdivisions < MIN_REGULAR_GRID_SUBDIVISIONS {
        return Err(format!(
            "regular-grid subdivisions must be at least {MIN_REGULAR_GRID_SUBDIVISIONS}"
        ));
    }
    if subdivisions > MAX_REGULAR_GRID_SUBDIVISIONS {
        return Err(format!(
            "regular-grid subdivisions cannot exceed {MAX_REGULAR_GRID_SUBDIVISIONS}"
        ));
    }
    regular_grid_point_count(subdivisions)
        .ok_or_else(|| "regular-grid point count overflows".to_owned())
        .map(|_| ())
}

/// Construct an empty project for a new desktop document.
///
/// This is intentionally different from [`default_regular_dataset`], which is
/// the populated editing fixture used by the headless/viewer workflows. A new
/// Qt document starts with declarations only: users add phases and grids.
pub fn empty_project_dataset() -> TabulatedTernaryDataset {
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
        phases: Vec::new(),
        properties: vec![PropertyDefinition {
            name: "T".into(),
            required: true,
            unit: "C".into(),
            line: 0,
        }],
        grids: Vec::new(),
        warnings: Vec::new(),
    }
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
        values: vec![TabulatedValue::missing(); row_count],
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

    /// Validate declarations and every object that exists. Empty phase and grid collections are valid drafts.
    pub fn validate_document_structure(&self) -> Result<(), String> {
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
            .any(|property| property.name.trim().is_empty() || property.unit.trim().is_empty())
            || self.properties.iter().enumerate().any(|(index, property)| {
                self.properties[..index]
                    .iter()
                    .any(|previous| previous.name == property.name)
            })
        {
            return Err("property names and units must be non-empty and unique".into());
        }
        let Some(temperature) = self.properties.iter().find(|property| property.name == "T") else {
            return Err("required property T is missing".into());
        };
        if !temperature.required {
            return Err("property T must remain required".into());
        }
        if self.missing_tokens.is_empty()
            || self
                .missing_tokens
                .iter()
                .any(|token| token.trim().is_empty() || token.chars().any(char::is_whitespace))
        {
            return Err("missing-value tokens must be non-blank single tokens".into());
        }
        if self.grids.iter().any(|grid| grid.name().trim().is_empty())
            || self.grids.iter().enumerate().any(|(index, grid)| {
                self.grids[..index]
                    .iter()
                    .any(|previous| previous.name() == grid.name())
            })
        {
            return Err("grid names must be non-empty and unique".into());
        }
        for grid in &self.grids {
            match grid {
                TabulatedGrid::Regular(value) => {
                    if value.subdivisions == 0 {
                        return Err(format!(
                            "grid {} must have a positive subdivision count",
                            grid.name()
                        ));
                    }
                    let expected =
                        regular_grid_point_count(value.subdivisions).ok_or_else(|| {
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
                    let _ = expected;
                }
                TabulatedGrid::Irregular(_) => {}
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
                if field.values.len() != grid.compositions().len() {
                    return Err(format!(
                        "grid {} field {}.{} has {} values; expected {}",
                        grid.name(),
                        field.phase_id.0,
                        field.property,
                        field.values.len(),
                        grid.compositions().len()
                    ));
                }
                for (row, value) in field.values.iter().enumerate() {
                    value.validate().map_err(|message| {
                        format!(
                            "grid {} field {}.{} row {}: {message}",
                            grid.name(),
                            field.phase_id.0,
                            field.property,
                            row + 1
                        )
                    })?;
                }
            }
            if grid.fields().iter().enumerate().any(|(index, field)| {
                grid.fields()[..index].iter().any(|prior| {
                    prior.phase_id == field.phase_id && prior.property == field.property
                })
            }) {
                return Err(format!(
                    "grid {} has duplicate phase/property fields",
                    grid.name()
                ));
            }
        }
        Ok(())
    }
    /// Backwards-compatible structural-validation name.
    pub fn validate_structure(&self) -> Result<(), String> {
        self.validate_document_structure()
    }

    /// Validate that the document can be represented safely as TCT.
    pub fn validate_saveable_document(&self) -> Result<(), String> {
        self.validate_document_structure()?;
        let unsafe_text = |value: &str| value.contains(['"', '\t', '\n', '\r']);
        if self.title.as_deref().is_some_and(unsafe_text)
            || self
                .components
                .iter()
                .any(|component| unsafe_text(&component.name))
            || self.phases.iter().any(|phase| unsafe_text(&phase.name))
            || self
                .properties
                .iter()
                .any(|property| unsafe_text(&property.name) || unsafe_text(&property.unit))
            || self.grids.iter().any(|grid| {
                unsafe_text(grid.name())
                    || grid.fields().iter().any(|field| {
                        unsafe_text(&field.property) || unsafe_text(&field.column_name)
                    })
            })
        {
            return Err("document declarations contain quote, tab, or newline text that cannot be represented safely in TCT".into());
        }
        Ok(())
    }

    /// Validate the additional inputs required by liquidus calculation.
    pub fn validate_calculation_readiness(&self) -> Result<(), String> {
        self.validate_document_structure()?;
        if self.phases.is_empty() {
            return Err("calculation requires at least one phase".into());
        }
        if self.grids.is_empty() {
            return Err("calculation requires at least one grid".into());
        }
        for phase in &self.phases {
            let temperature = self
                .grids
                .iter()
                .flat_map(TabulatedGrid::fields)
                .find(|field| field.phase_id == phase.id && field.property == "T");
            let Some(field) = temperature else {
                return Err(format!("phase {} has no required T field", phase.name));
            };
            if !field.values.iter().any(TabulatedValue::is_calculated) {
                return Err(format!(
                    "required Temperature values are missing for phase {}",
                    phase.name
                ));
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
    fn classified_values_enforce_finite_calculated_scalars_and_keep_states_distinct() {
        assert_eq!(
            TabulatedValue::calculated(1234.5)
                .unwrap()
                .calculated_value(),
            Some(1234.5)
        );
        assert!(TabulatedValue::calculated(f64::NAN).is_err());
        for value in [
            TabulatedValue::missing(),
            TabulatedValue::cut_off(),
            TabulatedValue::missing(),
        ] {
            assert!(value.validate().is_ok());
            assert!(value.calculated_value().is_none());
        }
        assert!(
            TabulatedValue {
                state: TabulatedValueState::CutOff,
                value: Some(1.0),
                extrapolation: None,
                note: None,
            }
            .validate()
            .is_err()
        );
    }

    #[test]
    fn state_tokens_parse_for_tct_and_excel_cells() {
        let missing = vec!["NA".into(), "MISSING".into()];
        assert_eq!(
            parse_tabulated_value_token("NE", &missing, false)
                .unwrap()
                .state,
            TabulatedValueState::Missing
        );
        let cutoff = parse_tabulated_value_token("CO:3000", &missing, false).unwrap();
        assert_eq!(cutoff.state, TabulatedValueState::CutOff);
        assert_eq!(cutoff.note.as_deref(), Some("3000"));
        assert_eq!(
            parse_tabulated_value_token("MISSING", &missing, false)
                .unwrap()
                .state,
            TabulatedValueState::Missing
        );
        assert!(parse_tabulated_value_token("", &missing, false).is_err());
        assert_eq!(
            parse_tabulated_value_token("", &missing, true)
                .unwrap()
                .state,
            TabulatedValueState::Missing
        );
    }

    #[test]
    fn regular_grid_resolution_bounds_and_point_counts() {
        assert_eq!(regular_grid_point_count(1), Some(3));
        assert_eq!(regular_grid_point_count(4), Some(15));
        assert_eq!(regular_grid_point_count(10), Some(66));
        assert_eq!(regular_grid_point_count(20), Some(231));
        assert_eq!(regular_grid_point_count(50), Some(1326));
        assert!(validate_new_regular_grid_subdivisions(0).is_err());
        assert!(validate_new_regular_grid_subdivisions(51).is_err());
        assert!(validate_new_regular_grid_subdivisions(50).is_ok());
    }

    #[test]
    fn empty_project_is_saveable_but_not_calculation_ready() {
        let dataset = empty_project_dataset();
        assert!(dataset.validate_document_structure().is_ok());
        assert!(dataset.validate_saveable_document().is_ok());
        assert!(dataset.validate_calculation_readiness().is_err());
        assert!(dataset.phases.is_empty());
        assert!(dataset.grids.is_empty());
        assert_eq!(dataset.properties[0].name, "T");
        assert_eq!(dataset.properties[0].unit, "C");
    }
    #[test]
    fn empty_project_dataset_has_only_required_declarations() {
        let dataset = empty_project_dataset();
        assert_eq!(dataset.title.as_deref(), Some("Untitled ternary system"));
        assert_eq!(
            dataset
                .components
                .iter()
                .map(|component| component.name.as_str())
                .collect::<Vec<_>>(),
            ["A", "B", "C"]
        );
        assert!(dataset.phases.is_empty());
        assert!(dataset.grids.is_empty());
        assert_eq!(dataset.properties.len(), 1);
        assert_eq!(dataset.properties[0].name, "T");
        assert!(dataset.properties[0].required);
        assert_eq!(dataset.properties[0].unit, "C");
    }

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
        assert!(grid.fields.iter().all(|field| {
            field
                .values
                .iter()
                .all(|value| value.state == TabulatedValueState::Missing)
        }));
        let text = crate::serialize_tct(&dataset, &crate::TctSerializeOptions::default()).unwrap();
        let round_trip = crate::parse_str(&text).unwrap();
        assert_eq!(round_trip.title, dataset.title);
        assert_eq!(
            round_trip.grids[0].compositions(),
            dataset.grids[0].compositions()
        );
    }
}
