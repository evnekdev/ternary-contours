#![allow(clippy::result_large_err)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

use ternary_contours::StablePhaseId;

use crate::parse_tsv_row;

use crate::model::{
    ComponentDefinition, CompositionColumns, FormatVersion, GridType, IrregularTabulatedGrid,
    PhaseDefinition, PropertyDefinition, RegularTabulatedGrid, RowOrder, SourceRange,
    TabulatedField, TabulatedGrid, TabulatedTernaryDataset,
};

const COMPOSITION_TOLERANCE: f64 = 1.0e-8;

// Diagnostics intentionally retain rich source context by value.

/// Source-aware syntax or validation failure in a `.tct` file.
#[derive(Clone, Debug)]
pub struct TctError {
    pub path: Option<PathBuf>,
    pub line: Option<usize>,
    pub section: Option<String>,
    pub grid: Option<String>,
    pub column: Option<String>,
    pub row: Option<usize>,
    pub token: Option<String>,
    pub expected: Option<String>,
    pub message: String,
}

impl TctError {
    fn new(line: usize, message: impl Into<String>) -> Self {
        Self {
            path: None,
            line: Some(line),
            section: None,
            grid: None,
            column: None,
            row: None,
            token: None,
            expected: None,
            message: message.into(),
        }
    }

    fn in_grid(mut self, grid: &str) -> Self {
        self.section = Some("grid".into());
        self.grid = Some(grid.into());
        self
    }

    fn with_column(mut self, column: &str) -> Self {
        self.column = Some(column.into());
        self
    }

    fn expected(mut self, expected: impl Into<String>) -> Self {
        self.expected = Some(expected.into());
        self
    }

    fn token(mut self, token: impl Into<String>) -> Self {
        self.token = Some(token.into());
        self
    }

    fn with_path(mut self, path: &Path) -> Self {
        self.path = Some(path.to_path_buf());
        self
    }
}

impl core::fmt::Display for TctError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        if let Some(path) = &self.path {
            write!(formatter, "{}", path.display())?;
        } else {
            formatter.write_str("TCT input")?;
        }
        if let Some(line) = self.line {
            write!(formatter, ":{line}")?;
        }
        write!(formatter, ": {}", self.message)?;
        if let Some(expected) = &self.expected {
            write!(formatter, " (expected {expected})")?;
        }
        Ok(())
    }
}

impl std::error::Error for TctError {}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SimpleSection {
    Components,
    Phases,
    Properties,
}

impl SimpleSection {
    const fn name(self) -> &'static str {
        match self {
            Self::Components => "components",
            Self::Phases => "phases",
            Self::Properties => "properties",
        }
    }
}

#[derive(Debug)]
struct GridBuilder {
    name: String,
    start_line: usize,
    metadata: BTreeMap<String, (String, usize)>,
    headers: Option<(Vec<String>, usize)>,
    awaiting_headers: bool,
    in_data: bool,
    rows: Vec<(usize, Vec<String>)>,
}

impl GridBuilder {
    fn new(name: String, start_line: usize) -> Self {
        Self {
            name,
            start_line,
            metadata: BTreeMap::new(),
            headers: None,
            awaiting_headers: false,
            in_data: false,
            rows: Vec::new(),
        }
    }

    fn error(&self, line: usize, message: impl Into<String>) -> TctError {
        TctError::new(line, message).in_grid(&self.name)
    }

    fn metadata(&self, key: &str) -> Option<(&str, usize)> {
        self.metadata
            .get(key)
            .map(|(value, line)| (value.as_str(), *line))
    }
}

#[derive(Clone)]
struct FieldColumn {
    phase: StablePhaseId,
    property: String,
    header: String,
    index: usize,
}

struct ParsedRows {
    compositions: Vec<[f64; 3]>,
    values: Vec<Vec<Option<f64>>>,
    lines: Vec<usize>,
}

/// Parse UTF-8 TCT text that has no associated source path.
pub fn parse_str(input: &str) -> Result<TabulatedTernaryDataset, TctError> {
    let mut header = None;
    let mut title = None;
    let mut composition_units = None;
    let mut missing_tokens = vec!["NA".to_owned()];
    let mut components = Vec::new();
    let mut phases = Vec::new();
    let mut properties = Vec::new();
    let mut simple_section: Option<SimpleSection> = None;
    let mut current_grid: Option<GridBuilder> = None;
    let mut raw_grids = Vec::new();
    let mut seen_sections = BTreeSet::new();

    for (offset, raw) in input.lines().enumerate() {
        let line = offset + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if header.is_none() {
            header = Some(parse_header(trimmed, line)?);
            continue;
        }
        if let Some(builder) = current_grid.as_mut() {
            if trimmed == "[/grid]" {
                if builder.awaiting_headers {
                    return Err(builder.error(line, "`columns:` is missing its TSV header row"));
                }
                if !builder.in_data {
                    return Err(builder.error(line, "grid is missing its `data:` marker"));
                }
                raw_grids.push(current_grid.take().expect("checked current grid"));
                continue;
            }
            if builder.in_data {
                builder.rows.push((
                    line,
                    parse_tsv_row(line, raw)
                        .cells
                        .into_iter()
                        .map(|cell| cell.text)
                        .collect(),
                ));
                continue;
            }
            if builder.awaiting_headers {
                let headers = raw
                    .split('\t')
                    .map(|cell| cell.trim().to_owned())
                    .collect::<Vec<_>>();
                if headers.iter().any(String::is_empty) || has_duplicates(&headers) {
                    return Err(builder.error(line, "column headers must be non-blank and unique"));
                }
                builder.headers = Some((headers, line));
                builder.awaiting_headers = false;
                continue;
            }
            if trimmed == "columns:" {
                if builder.headers.is_some() {
                    return Err(builder.error(line, "grid declares `columns:` more than once"));
                }
                builder.awaiting_headers = true;
                continue;
            }
            if trimmed == "data:" {
                if builder.headers.is_none() {
                    return Err(
                        builder.error(line, "`data:` must follow `columns:` and its TSV header")
                    );
                }
                builder.in_data = true;
                continue;
            }
            let (key, value) = split_assignment(trimmed, line)?;
            if builder
                .metadata
                .insert(key.to_owned(), (unquote(value, line)?, line))
                .is_some()
            {
                return Err(builder.error(line, format!("duplicate grid setting `{key}`")));
            }
            continue;
        }
        if let Some(section) = simple_section {
            if trimmed == format!("[/{name}]", name = section.name()) {
                simple_section = None;
                continue;
            }
            match section {
                SimpleSection::Components => components.push(ComponentDefinition {
                    name: unquote(trimmed, line)?,
                    line,
                }),
                SimpleSection::Phases => {
                    let (name, id) = split_assignment(trimmed, line)?;
                    let id = id.parse::<u32>().map_err(|_| {
                        TctError::new(line, "phase ID is not a non-negative integer")
                            .token(id)
                            .expected("an integer phase ID")
                    })?;
                    phases.push(PhaseDefinition {
                        name: unquote(name, line)?,
                        id: StablePhaseId(id),
                        line,
                    });
                }
                SimpleSection::Properties => {
                    let words = trimmed.split_whitespace().collect::<Vec<_>>();
                    if words.len() < 3 {
                        return Err(TctError::new(line, "invalid property declaration")
                            .expected("`name required|optional unit`"));
                    }
                    let required = match words[1] {
                        "required" => true,
                        "optional" => false,
                        other => {
                            return Err(TctError::new(line, "invalid property requirement")
                                .token(other)
                                .expected("`required` or `optional`"));
                        }
                    };
                    properties.push(PropertyDefinition {
                        name: words[0].to_owned(),
                        required,
                        unit: words[2..].join(" "),
                        line,
                    });
                }
            }
            continue;
        }
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Some(name) = trimmed
                .strip_prefix("[grid ")
                .and_then(|value| value.strip_suffix(']'))
            {
                let name = unquote(name.trim(), line)?;
                if name.is_empty() {
                    return Err(TctError::new(line, "grid name must not be empty"));
                }
                current_grid = Some(GridBuilder::new(name, line));
                continue;
            }
            let section = match trimmed {
                "[components]" => SimpleSection::Components,
                "[phases]" => SimpleSection::Phases,
                "[properties]" => SimpleSection::Properties,
                _ => {
                    return Err(
                        TctError::new(line, "unknown or malformed section header").token(trimmed)
                    );
                }
            };
            if !seen_sections.insert(section.name()) {
                return Err(TctError::new(
                    line,
                    format!("duplicate `[{}]` section", section.name()),
                ));
            }
            simple_section = Some(section);
            continue;
        }
        let (key, value) = split_assignment(trimmed, line)?;
        let value = unquote(value, line)?;
        match key {
            "title" if title.replace(value.clone()).is_none() => {}
            "composition_units" if composition_units.replace(value.clone()).is_none() => {}
            "default_missing" => missing_tokens = vec![value],
            "missing_tokens" => {
                let tokens = value
                    .split_whitespace()
                    .map(str::to_owned)
                    .collect::<Vec<_>>();
                if tokens.is_empty() {
                    return Err(TctError::new(
                        line,
                        "`missing_tokens` must contain at least one token",
                    ));
                }
                missing_tokens = tokens;
            }
            "title" | "composition_units" => {
                return Err(TctError::new(
                    line,
                    format!("duplicate global declaration `{key}`"),
                ));
            }
            _ => {
                return Err(TctError::new(
                    line,
                    format!("unknown global declaration `{key}`"),
                ));
            }
        }
    }
    let version = header.ok_or_else(|| TctError::new(1, "missing `TCT 1.0` header"))?;
    if let Some(section) = simple_section {
        return Err(TctError::new(
            input.lines().count(),
            format!("unclosed `[{}]` section", section.name()),
        ));
    }
    if let Some(builder) = current_grid {
        return Err(builder.error(input.lines().count(), "unclosed grid section"));
    }
    if components.len() != 3 {
        return Err(TctError::new(
            1,
            format!(
                "exactly three components are required; found {}",
                components.len()
            ),
        ));
    }
    if components.iter().any(|component| component.name.is_empty())
        || has_duplicates_by(&components, |component| component.name.clone())
    {
        return Err(TctError::new(
            components.first().map_or(1, |component| component.line),
            "component names must be non-empty and unique",
        ));
    }
    let components: [ComponentDefinition; 3] =
        components.try_into().expect("checked component length");
    validate_declarations(&phases, &properties)?;

    let mut grids = Vec::with_capacity(raw_grids.len());
    let mut grid_names = BTreeSet::new();
    let mut defined_fields = BTreeSet::new();
    for builder in raw_grids {
        if !grid_names.insert(builder.name.clone()) {
            return Err(builder.error(builder.start_line, "duplicate grid name"));
        }
        let grid = finalize_grid(builder, &components, &phases, &properties, &missing_tokens)?;
        for field in grid.fields() {
            if !defined_fields.insert((field.phase_id, field.property.clone())) {
                return Err(TctError::new(
                    1,
                    format!(
                        "duplicate field definition for phase {:?} property `{}`",
                        field.phase_id, field.property
                    ),
                ));
            }
        }
        grids.push(grid);
    }
    if grids.is_empty() {
        return Err(TctError::new(1, "at least one grid is required"));
    }
    for phase in &phases {
        if !defined_fields.contains(&(phase.id, "T".to_owned())) {
            return Err(TctError::new(
                phase.line,
                format!("phase `{}` has no required `T` field", phase.name),
            ));
        }
    }
    Ok(TabulatedTernaryDataset {
        source_path: None,
        version,
        title,
        composition_units,
        missing_tokens,
        components,
        phases,
        properties,
        grids,
        warnings: Vec::new(),
    })
}

/// Read UTF-8 text and associate parser diagnostics with `path`.
pub fn parse_path(path: impl AsRef<Path>) -> Result<TabulatedTernaryDataset, TctError> {
    let path = path.as_ref();
    let input = fs::read_to_string(path).map_err(|error| TctError {
        path: Some(path.to_path_buf()),
        line: None,
        section: None,
        grid: None,
        column: None,
        row: None,
        token: None,
        expected: None,
        message: format!("could not read UTF-8 input: {error}"),
    })?;
    let mut dataset = parse_str(&input).map_err(|error| error.with_path(path))?;
    dataset.source_path = Some(path.to_path_buf());
    Ok(dataset)
}

fn parse_header(value: &str, line: usize) -> Result<FormatVersion, TctError> {
    let words = value.split_whitespace().collect::<Vec<_>>();
    if words.len() != 2 || words[0] != "TCT" {
        return Err(TctError::new(line, "missing or malformed TCT header").expected("`TCT 1.0`"));
    }
    let (major, minor) = words[1]
        .split_once('.')
        .ok_or_else(|| TctError::new(line, "malformed TCT format version").expected("`TCT 1.0`"))?;
    let version = FormatVersion {
        major: major
            .parse()
            .map_err(|_| TctError::new(line, "malformed TCT major version"))?,
        minor: minor
            .parse()
            .map_err(|_| TctError::new(line, "malformed TCT minor version"))?,
    };
    if version != (FormatVersion { major: 1, minor: 0 }) {
        return Err(
            TctError::new(line, format!("unsupported TCT version {version}")).expected("`TCT 1.0`"),
        );
    }
    Ok(version)
}

fn split_assignment(value: &str, line: usize) -> Result<(&str, &str), TctError> {
    let (key, value) = value.split_once('=').ok_or_else(|| {
        TctError::new(line, "expected a key/value declaration").expected("`key = value`")
    })?;
    let key = key.trim();
    let value = value.trim();
    if key.is_empty() || value.is_empty() {
        return Err(
            TctError::new(line, "key/value declarations must not be blank")
                .expected("`key = value`"),
        );
    }
    Ok((key, value))
}

fn unquote(value: &str, line: usize) -> Result<String, TctError> {
    let value = value.trim();
    if value.starts_with('"') || value.ends_with('"') {
        if value.len() < 2 || !value.starts_with('"') || !value.ends_with('"') {
            return Err(TctError::new(line, "unterminated quoted name"));
        }
        Ok(value[1..value.len() - 1].to_owned())
    } else {
        Ok(value.to_owned())
    }
}

fn validate_declarations(
    phases: &[PhaseDefinition],
    properties: &[PropertyDefinition],
) -> Result<(), TctError> {
    if phases.is_empty() {
        return Err(TctError::new(1, "at least one phase is required"));
    }
    if phases.iter().any(|phase| phase.name.is_empty())
        || has_duplicates_by(phases, |phase| phase.name.clone())
    {
        return Err(TctError::new(
            phases[0].line,
            "phase names must be non-empty and unique",
        ));
    }
    if has_duplicates_by(phases, |phase| phase.id) {
        return Err(TctError::new(phases[0].line, "phase IDs must be unique"));
    }
    if has_duplicates_by(properties, |property| property.name.clone()) {
        return Err(TctError::new(
            properties.first().map_or(1, |property| property.line),
            "property names must be unique",
        ));
    }
    if !properties
        .iter()
        .any(|property| property.name == "T" && property.required)
    {
        return Err(TctError::new(
            1,
            "properties must declare `T required <unit>`",
        ));
    }
    Ok(())
}
fn finalize_grid(
    builder: GridBuilder,
    components: &[ComponentDefinition; 3],
    phases: &[PhaseDefinition],
    properties: &[PropertyDefinition],
    missing_tokens: &[String],
) -> Result<TabulatedGrid, TctError> {
    let (type_text, type_line) = builder.metadata("type").ok_or_else(|| {
        builder.error(
            builder.start_line,
            "grid is missing `type = regular|irregular`",
        )
    })?;
    let kind = match type_text {
        "regular" => GridType::Regular,
        "irregular" => GridType::Irregular,
        other => {
            return Err(builder
                .error(type_line, "invalid grid type")
                .token(other)
                .expected("`regular` or `irregular`"));
        }
    };
    let declared_properties = builder
        .metadata("properties")
        .map(|(value, _)| {
            value
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if declared_properties.is_empty() {
        return Err(builder.error(builder.start_line, "grid is missing `properties = ...`"));
    }
    let (headers, header_line) = builder.headers.as_ref().expect("closed grids have headers");
    let mut component_indexes = [None; 3];
    for (index, header) in headers.iter().enumerate() {
        for (component_index, component) in components.iter().enumerate() {
            if header == &component.name {
                component_indexes[component_index] = Some(index);
            }
        }
    }
    let supplied_compositions = component_indexes.iter().all(Option::is_some);
    if component_indexes.iter().any(Option::is_some) && !supplied_compositions {
        return Err(builder.error(
            *header_line,
            "composition columns must include all three declared components",
        ));
    }
    let local_phase =
        match builder.metadata("phase") {
            Some((name, _)) => Some(phase_by_name(phases, name).ok_or_else(|| {
                builder.error(builder.start_line, format!("unknown phase `{name}`"))
            })?),
            None => None,
        };
    let mut field_columns = Vec::new();
    for (index, header) in headers.iter().enumerate() {
        if component_indexes.contains(&Some(index)) {
            continue;
        }
        let (phase, property) = if let Some((phase_name, property)) = header.split_once('.') {
            if property.contains('.') || phase_name.is_empty() || property.is_empty() {
                return Err(builder
                    .error(*header_line, "malformed qualified field column")
                    .with_column(header));
            }
            let phase = phase_by_name(phases, phase_name).ok_or_else(|| {
                builder
                    .error(*header_line, format!("unknown phase `{phase_name}`"))
                    .with_column(header)
            })?;
            if let Some(local) = local_phase
                && local.id != phase.id
            {
                return Err(builder
                    .error(
                        *header_line,
                        "phase/property column conflicts with grid `phase` declaration",
                    )
                    .with_column(header));
            }
            (phase, property.to_owned())
        } else {
            let phase = local_phase.ok_or_else(|| {
                builder
                    .error(
                        *header_line,
                        "unqualified property columns require `phase = ...`",
                    )
                    .with_column(header)
            })?;
            (phase, header.clone())
        };
        if !properties
            .iter()
            .any(|candidate| candidate.name == property)
        {
            return Err(builder
                .error(*header_line, format!("unknown property `{property}`"))
                .with_column(header));
        }
        if !declared_properties
            .iter()
            .any(|candidate| candidate == &property)
        {
            return Err(builder
                .error(
                    *header_line,
                    format!("field property `{property}` is absent from `properties = ...`"),
                )
                .with_column(header));
        }
        field_columns.push(FieldColumn {
            phase: phase.id,
            property,
            header: header.clone(),
            index,
        });
    }
    if field_columns.is_empty()
        || has_duplicates_by(&field_columns, |field| {
            (field.phase, field.property.clone())
        })
    {
        return Err(builder.error(
            *header_line,
            "grid must provide unique phase/property field columns",
        ));
    }
    for property in &declared_properties {
        if !properties
            .iter()
            .any(|candidate| candidate.name == *property)
        {
            return Err(builder.error(
                builder.start_line,
                format!("unknown declared property `{property}`"),
            ));
        }
    }
    let rows = parse_rows(
        &builder,
        headers,
        &field_columns,
        component_indexes,
        properties,
        missing_tokens,
    )?;
    let source = SourceRange {
        first_line: builder.start_line,
        last_line: rows.lines.last().copied().unwrap_or(builder.start_line),
    };
    match kind {
        GridType::Regular => {
            finalize_regular(builder, source, rows, field_columns, supplied_compositions)
        }
        GridType::Irregular => {
            finalize_irregular(builder, source, rows, field_columns, supplied_compositions)
        }
    }
}

fn parse_rows(
    builder: &GridBuilder,
    headers: &[String],
    fields: &[FieldColumn],
    component_indexes: [Option<usize>; 3],
    _properties: &[PropertyDefinition],
    missing_tokens: &[String],
) -> Result<ParsedRows, TctError> {
    let mut compositions = Vec::with_capacity(builder.rows.len());
    let mut values = vec![Vec::with_capacity(builder.rows.len()); fields.len()];
    let mut lines = Vec::with_capacity(builder.rows.len());
    for (line, row) in &builder.rows {
        if row.len() != headers.len() {
            return Err(builder
                .error(
                    *line,
                    format!(
                        "wrong data row width: expected {}, found {}",
                        headers.len(),
                        row.len()
                    ),
                )
                .expected("the same number of tab-separated cells as `columns:`"));
        }
        let composition = if component_indexes.iter().all(Option::is_some) {
            let mut composition = [0.0; 3];
            for (component, index) in component_indexes.into_iter().enumerate() {
                let index = index.expect("all indexes checked");
                composition[component] =
                    parse_number(&row[index], *line, builder, &headers[index])?;
            }
            validate_composition(composition, *line, builder)?;
            composition
        } else {
            [f64::NAN; 3]
        };
        for (field_index, field) in fields.iter().enumerate() {
            let token = row[field.index].trim();
            let value = if missing_tokens.iter().any(|missing| missing == token) {
                None
            } else {
                if token.is_empty() {
                    return Err(builder
                        .error(*line, "blank cells are not allowed in TSV data")
                        .with_column(&field.header));
                }
                Some(parse_number(token, *line, builder, &field.header)?)
            };
            values[field_index].push(value);
        }
        compositions.push(composition);
        lines.push(*line);
    }
    Ok(ParsedRows {
        compositions,
        values,
        lines,
    })
}
fn finalize_regular(
    builder: GridBuilder,
    source: SourceRange,
    mut rows: ParsedRows,
    fields: Vec<FieldColumn>,
    supplied_compositions: bool,
) -> Result<TabulatedGrid, TctError> {
    let (subdivisions_text, subdivisions_line) =
        builder.metadata("subdivisions").ok_or_else(|| {
            builder.error(
                builder.start_line,
                "regular grid is missing `subdivisions = N`",
            )
        })?;
    let subdivisions = subdivisions_text.parse::<usize>().map_err(|_| {
        builder
            .error(subdivisions_line, "invalid regular-grid subdivision count")
            .token(subdivisions_text)
    })?;
    if subdivisions == 0 {
        return Err(builder.error(
            subdivisions_line,
            "regular-grid subdivisions must be positive",
        ));
    }
    let (order_text, order_line) = builder.metadata("order").ok_or_else(|| {
        builder.error(
            builder.start_line,
            "regular grid is missing `order = canonical|compositions`",
        )
    })?;
    let order = match order_text {
        "canonical" => RowOrder::Canonical,
        "compositions" => RowOrder::Compositions,
        other => {
            return Err(builder
                .error(order_line, "invalid regular-grid order")
                .token(other));
        }
    };
    let (composition_text, composition_line) =
        builder.metadata("composition_columns").ok_or_else(|| {
            builder.error(
                builder.start_line,
                "regular grid is missing `composition_columns = none|guidance|authoritative`",
            )
        })?;
    let composition_columns = match composition_text {
        "none" => CompositionColumns::None,
        "guidance" => CompositionColumns::Guidance,
        "authoritative" => CompositionColumns::Authoritative,
        other => {
            return Err(builder
                .error(composition_line, "invalid composition-column mode")
                .token(other));
        }
    };
    let count = vertex_count(subdivisions)
        .ok_or_else(|| builder.error(subdivisions_line, "regular-grid vertex count overflow"))?;
    if rows.compositions.len() != count {
        return Err(builder.error(
            source.last_line,
            format!(
                "regular grid has {} rows; expected {count}",
                rows.compositions.len()
            ),
        ));
    }
    match composition_columns {
        CompositionColumns::None => {
            if supplied_compositions {
                return Err(builder.error(
                    source.first_line,
                    "composition columns are not allowed with `composition_columns = none`",
                ));
            }
            rows.compositions = canonical_compositions(subdivisions);
        }
        CompositionColumns::Guidance => {
            if !supplied_compositions {
                return Err(builder.error(
                    source.first_line,
                    "guidance composition columns require all three component columns",
                ));
            }
            let expected = canonical_compositions(subdivisions);
            for (index, (supplied, expected)) in rows.compositions.iter().zip(&expected).enumerate()
            {
                let residual = max_residual(*supplied, *expected);
                if residual > COMPOSITION_TOLERANCE {
                    return Err(builder.error(rows.lines[index], format!("composition guidance mismatch: expected {:?}, supplied {:?}, residual {residual:e}, tolerance {COMPOSITION_TOLERANCE:e}", expected, supplied)));
                }
            }
            rows.compositions = expected;
        }
        CompositionColumns::Authoritative => {
            if !supplied_compositions {
                return Err(builder.error(
                    source.first_line,
                    "authoritative composition columns require all three component columns",
                ));
            }
            let mut seen = vec![false; count];
            let mut values = vec![vec![None; count]; fields.len()];
            let mut lines = vec![0; count];
            for row in 0..count {
                let index =
                    regular_index(rows.compositions[row], subdivisions).ok_or_else(|| {
                        builder.error(
                            rows.lines[row],
                            "composition is not on the declared regular lattice",
                        )
                    })?;
                if seen[index] {
                    return Err(builder.error(rows.lines[row], "duplicate regular-grid point"));
                }
                seen[index] = true;
                lines[index] = rows.lines[row];
                for (field, destination) in values.iter_mut().enumerate() {
                    destination[index] = rows.values[field][row];
                }
            }
            if seen.iter().any(|seen| !seen) {
                return Err(builder.error(
                    source.last_line,
                    "regular grid is missing one or more required lattice points",
                ));
            }
            rows.compositions = canonical_compositions(subdivisions);
            rows.values = values;
            rows.lines = lines;
        }
    }
    let fields = fields
        .into_iter()
        .enumerate()
        .map(|(index, field)| TabulatedField {
            phase_id: field.phase,
            property: field.property,
            column_name: field.header,
            values: rows.values[index].clone(),
            row_lines: rows.lines.clone(),
        })
        .collect();
    Ok(TabulatedGrid::Regular(RegularTabulatedGrid {
        name: builder.name,
        source,
        subdivisions,
        order,
        composition_columns,
        compositions: rows.compositions,
        fields,
    }))
}

fn finalize_irregular(
    builder: GridBuilder,
    source: SourceRange,
    rows: ParsedRows,
    fields: Vec<FieldColumn>,
    supplied_compositions: bool,
) -> Result<TabulatedGrid, TctError> {
    if !supplied_compositions {
        return Err(builder.error(
            source.first_line,
            "irregular grids require authoritative component columns",
        ));
    }
    if builder.metadata("subdivisions").is_some() {
        return Err(builder.error(
            source.first_line,
            "irregular grids must not declare `subdivisions`",
        ));
    }
    match builder.metadata("composition_columns") {
        Some(("authoritative", _)) => {}
        Some((_, line)) => {
            return Err(builder.error(
                line,
                "irregular grid composition columns must be authoritative",
            ));
        }
        None => {
            return Err(builder.error(
                source.first_line,
                "irregular grids require `composition_columns = authoritative`",
            ));
        }
    }
    if rows.compositions.len() < 3 {
        return Err(builder.error(
            source.last_line,
            "irregular grids require at least three distinct points",
        ));
    }
    for (right, composition) in rows.compositions.iter().enumerate() {
        if rows.compositions[..right]
            .iter()
            .any(|left| max_residual(*left, *composition) <= COMPOSITION_TOLERANCE)
        {
            return Err(builder.error(rows.lines[right], "duplicate irregular-grid point"));
        }
    }
    if !non_collinear(&rows.compositions) {
        return Err(builder.error(source.last_line, "irregular-grid points are collinear"));
    }
    let fields = fields
        .into_iter()
        .enumerate()
        .map(|(index, field)| TabulatedField {
            phase_id: field.phase,
            property: field.property,
            column_name: field.header,
            values: rows.values[index].clone(),
            row_lines: rows.lines.clone(),
        })
        .collect();
    Ok(TabulatedGrid::Irregular(IrregularTabulatedGrid {
        name: builder.name,
        source,
        compositions: rows.compositions,
        fields,
    }))
}
fn parse_number(
    token: &str,
    line: usize,
    builder: &GridBuilder,
    column: &str,
) -> Result<f64, TctError> {
    let value = token.trim().parse::<f64>().map_err(|_| {
        builder
            .error(line, "invalid number")
            .with_column(column)
            .token(token)
    })?;
    if !value.is_finite() {
        return Err(builder
            .error(line, "numeric values must be finite")
            .with_column(column)
            .token(token));
    }
    Ok(value)
}

fn validate_composition(
    composition: [f64; 3],
    line: usize,
    builder: &GridBuilder,
) -> Result<(), TctError> {
    if composition
        .iter()
        .any(|value| *value < -COMPOSITION_TOLERANCE)
    {
        return Err(builder.error(
            line,
            "composition components must be non-negative within tolerance",
        ));
    }
    let sum = composition.into_iter().sum::<f64>();
    if (sum - 1.0).abs() > COMPOSITION_TOLERANCE {
        return Err(builder.error(
            line,
            format!("composition must sum to one; found {sum:.12}"),
        ));
    }
    Ok(())
}

fn phase_by_name<'a>(phases: &'a [PhaseDefinition], name: &str) -> Option<&'a PhaseDefinition> {
    phases.iter().find(|phase| phase.name == name)
}

fn vertex_count(subdivisions: usize) -> Option<usize> {
    subdivisions
        .checked_add(1)?
        .checked_mul(subdivisions.checked_add(2)?)
        .map(|value| value / 2)
}

fn canonical_compositions(subdivisions: usize) -> Vec<[f64; 3]> {
    let denominator = subdivisions as f64;
    let mut result = Vec::with_capacity(vertex_count(subdivisions).expect("already validated"));
    for i in 0..=subdivisions {
        for j in 0..=subdivisions - i {
            result.push([
                i as f64 / denominator,
                j as f64 / denominator,
                (subdivisions - i - j) as f64 / denominator,
            ]);
        }
    }
    result
}

fn regular_index(composition: [f64; 3], subdivisions: usize) -> Option<usize> {
    let scaled = composition.map(|value| value * subdivisions as f64);
    let rounded = scaled.map(f64::round);
    if max_residual(scaled, rounded) > COMPOSITION_TOLERANCE
        || rounded.iter().any(|value| *value < 0.0)
    {
        return None;
    }
    let [i, j, k] = rounded.map(|value| value as usize);
    if i.checked_add(j)?.checked_add(k)? != subdivisions {
        return None;
    }
    i.checked_mul(subdivisions.checked_add(1)?)?
        .checked_sub(i.checked_mul(i.saturating_sub(1))? / 2)?
        .checked_add(j)
}

fn max_residual(left: [f64; 3], right: [f64; 3]) -> f64 {
    left.into_iter()
        .zip(right)
        .map(|(left, right)| (left - right).abs())
        .fold(0.0, f64::max)
}

fn non_collinear(points: &[[f64; 3]]) -> bool {
    let first = points[0];
    points.iter().skip(1).any(|second| {
        points.iter().skip(2).any(|third| {
            ((second[0] - first[0]) * (third[1] - first[1])
                - (second[1] - first[1]) * (third[0] - first[0]))
                .abs()
                > COMPOSITION_TOLERANCE
        })
    })
}

fn has_duplicates<T: Ord>(values: &[T]) -> bool {
    let mut values = values.iter().collect::<Vec<_>>();
    values.sort();
    values.windows(2).any(|pair| pair[0] == pair[1])
}

fn has_duplicates_by<T, K: Ord>(values: &[T], mut key: impl FnMut(&T) -> K) -> bool {
    let mut keys = values.iter().map(&mut key).collect::<Vec<_>>();
    keys.sort();
    keys.windows(2).any(|pair| pair[0] == pair[1])
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "TCT 1.0\n\n[components]\nA\nB\nC\n[/components]\n\n[phases]\nalpha = 1\nbeta = 2\n[/phases]\n\n[properties]\nT required K\nactivity optional 1\n[/properties]\n\n[grid shared]\ntype = regular\nsubdivisions = 1\norder = canonical\ncomposition_columns = none\nproperties = T activity\ncolumns:\nalpha.T\tbeta.T\talpha.activity\ndata:\n100\t90\t0.2\n90\t100\t0.3\n80\t80\tNA\n[/grid]\n";

    #[test]
    fn parses_minimal_regular_tct_with_lf_and_crlf() {
        let lf = parse_str(MINIMAL).unwrap();
        let crlf = parse_str(&MINIMAL.replace('\n', "\r\n")).unwrap();
        assert_eq!(lf, crlf);
        assert_eq!(lf.grids.len(), 1);
    }

    #[test]
    fn supports_utf8_and_quoted_phase_names() {
        let phase = "\u{03B1} phase";
        let input = MINIMAL
            .replace("alpha = 1", &format!("\"{phase}\" = 1"))
            .replace("alpha.T", &format!("{phase}.T"))
            .replace("alpha.activity", &format!("{phase}.activity"));
        let parsed = parse_str(&input).unwrap();
        assert_eq!(parsed.phases[0].name, phase);
    }
}
