//! Deterministic TCT 1.0 serialization for generated and edited datasets.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    CompositionColumns, GridType, RowOrder, TabulatedField, TabulatedGrid, TabulatedTernaryDataset,
};

/// Formatting choices shared by TSV copy and TCT saving.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NumericFormat {
    /// Decimal places emitted for finite values.
    pub decimal_places: usize,
}

impl Default for NumericFormat {
    fn default() -> Self {
        Self { decimal_places: 6 }
    }
}

impl NumericFormat {
    pub fn format(&self, value: f64) -> String {
        format!("{value:.precision$}", precision = self.decimal_places)
    }
}

/// Serializer behaviour for a saved TCT document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TctSerializeOptions {
    pub numeric: NumericFormat,
    pub missing_token: String,
}

impl Default for TctSerializeOptions {
    fn default() -> Self {
        Self {
            numeric: NumericFormat::default(),
            missing_token: "NA".into(),
        }
    }
}

/// Serialization error that deliberately prevents a misleading output file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SerializeError(pub String);

impl fmt::Display for SerializeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for SerializeError {}

/// Serialize a neutral dataset in stable section, grid, field, and row order.
pub fn serialize_tct(
    dataset: &TabulatedTernaryDataset,
    options: &TctSerializeOptions,
) -> Result<String, SerializeError> {
    dataset.validate_structure().map_err(SerializeError)?;
    if options.missing_token.trim().is_empty()
        || options.missing_token.contains(char::is_whitespace)
    {
        return Err(SerializeError(
            "configured TCT missing token must be a single non-blank token".into(),
        ));
    }
    let mut output = String::from("TCT 1.0\n\n");
    if let Some(title) = &dataset.title {
        output.push_str("title = ");
        output.push_str(&quoted(title)?);
        output.push('\n');
    }
    if let Some(units) = &dataset.composition_units {
        output.push_str("composition_units = ");
        output.push_str(&quoted(units)?);
        output.push('\n');
    }
    let missing = if options.missing_token == "NA" {
        dataset
            .missing_tokens
            .first()
            .unwrap_or(&options.missing_token)
    } else {
        &options.missing_token
    };
    output.push_str("default_missing = ");
    output.push_str(missing);
    output.push_str("\n\n[components]\n");
    for component in &dataset.components {
        output.push_str(&quoted(&component.name)?);
        output.push('\n');
    }
    output.push_str("[/components]\n\n[phases]\n");
    for phase in &dataset.phases {
        output.push_str(&quoted(&phase.name)?);
        output.push_str(" = ");
        output.push_str(&phase.id.0.to_string());
        output.push('\n');
    }
    output.push_str("[/phases]\n\n[properties]\n");
    for property in &dataset.properties {
        output.push_str(&property.name);
        output.push(' ');
        output.push_str(if property.required {
            "required"
        } else {
            "optional"
        });
        output.push(' ');
        output.push_str(&property.unit);
        output.push('\n');
    }
    output.push_str("[/properties]\n");

    for grid in &dataset.grids {
        output.push('\n');
        serialize_grid(&mut output, dataset, grid, options, missing)?;
    }
    Ok(output)
}

fn serialize_grid(
    output: &mut String,
    dataset: &TabulatedTernaryDataset,
    grid: &TabulatedGrid,
    options: &TctSerializeOptions,
    missing: &str,
) -> Result<(), SerializeError> {
    output.push_str("[grid ");
    output.push_str(&quoted(grid.name())?);
    output.push_str("]\n");
    output.push_str("type = ");
    output.push_str(match grid.grid_type() {
        GridType::Regular => "regular",
        GridType::Irregular => "irregular",
    });
    output.push('\n');

    let (subdivisions, order, composition_mode) = match grid {
        TabulatedGrid::Regular(regular) => (
            Some(regular.subdivisions),
            Some(regular.order),
            regular.composition_columns,
        ),
        TabulatedGrid::Irregular(_) => (None, None, CompositionColumns::Authoritative),
    };
    if let Some(subdivisions) = subdivisions {
        output.push_str("subdivisions = ");
        output.push_str(&subdivisions.to_string());
        output.push('\n');
        output.push_str("order = ");
        output.push_str(match order.expect("regular grid has row order") {
            RowOrder::Canonical => "canonical",
            RowOrder::Compositions => "compositions",
        });
        output.push('\n');
    }
    output.push_str("composition_columns = ");
    output.push_str(match composition_mode {
        CompositionColumns::None => "none",
        CompositionColumns::Guidance => "guidance",
        CompositionColumns::Authoritative => "authoritative",
    });
    output.push('\n');

    let fields = grid.fields().to_vec();
    let mut field_properties = fields
        .iter()
        .map(|field| field.property.clone())
        .collect::<Vec<_>>();
    field_properties.dedup();
    output.push_str("properties = ");
    output.push_str(&field_properties.join(" "));
    output.push_str("\ncolumns:\n");

    let include_compositions = !matches!(composition_mode, CompositionColumns::None);
    let mut headers = Vec::new();
    if include_compositions {
        headers.extend(
            dataset
                .components
                .iter()
                .map(|component| component.name.clone()),
        );
    }
    headers.extend(fields.iter().map(|field| field_header(dataset, field)));
    output.push_str(&headers.join("\t"));
    output.push_str("\ndata:\n");
    for (row, composition) in grid.compositions().iter().enumerate() {
        let mut cells = Vec::with_capacity(headers.len());
        if include_compositions {
            cells.extend(composition.map(|value| options.numeric.format(value)));
        }
        for field in &fields {
            cells.push(
                field
                    .values
                    .get(row)
                    .map(|value| {
                        value.token_with_format(|number| options.numeric.format(number), missing)
                    })
                    .unwrap_or_else(|| missing.to_owned()),
            );
        }
        output.push_str(&cells.join("\t"));
        output.push('\n');
    }
    output.push_str("[/grid]\n");
    Ok(())
}

fn field_header(dataset: &TabulatedTernaryDataset, field: &TabulatedField) -> String {
    let phase = dataset
        .phases
        .iter()
        .find(|phase| phase.id == field.phase_id)
        .map(|phase| phase.name.as_str())
        .unwrap_or("unknown");
    format!("{phase}.{}", field.property)
}

fn quoted(value: &str) -> Result<String, SerializeError> {
    if value.contains('"') || value.contains('\n') || value.contains('\r') {
        return Err(SerializeError(
            "TCT names cannot contain quote or newline characters".into(),
        ));
    }
    Ok(format!("\"{value}\""))
}

/// Write a finished document via a sibling temporary file then rename it.
///
/// Existing targets are replaced only after the temporary write succeeds. The
/// replacement is atomic on platforms that allow rename-overwrite; Windows
/// falls back to a remove-and-rename after the durable temporary write.
pub fn save_tct_atomic(path: impl AsRef<Path>, contents: &str) -> Result<(), SerializeError> {
    let path = path.as_ref();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| SerializeError(error.to_string()))?
        .as_nanos();
    let temporary = parent.join(format!(
        ".{}.{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("tct"),
        stamp
    ));
    fs::write(&temporary, contents)
        .map_err(|error| SerializeError(format!("could not write temporary TCT file: {error}")))?;
    if let Err(first_error) = fs::rename(&temporary, path) {
        if path.exists() {
            fs::remove_file(path).map_err(|error| {
                SerializeError(format!(
                    "could not replace existing TCT file after save: {error}"
                ))
            })?;
            fs::rename(&temporary, path).map_err(|error| {
                SerializeError(format!(
                    "could not move temporary TCT file into place: {error}"
                ))
            })?;
        } else {
            let _ = fs::remove_file(&temporary);
            return Err(SerializeError(format!(
                "could not save TCT file: {first_error}"
            )));
        }
    }
    Ok(())
}

/// Keep temporary file names testable without exposing filesystem details.
pub fn temporary_save_path(path: &Path, suffix: u128) -> PathBuf {
    path.parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(
            ".{}.{}.tmp",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("tct"),
            suffix
        ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse_str;

    #[test]
    fn serialization_is_deterministic_and_round_trips() {
        let dataset = parse_str(include_str!("../fixtures/minimal-regular.tct")).unwrap();
        let first = serialize_tct(&dataset, &TctSerializeOptions::default()).unwrap();
        let second = serialize_tct(&dataset, &TctSerializeOptions::default()).unwrap();
        assert_eq!(first, second);
        let reparsed = parse_str(&first).unwrap();
        assert_eq!(reparsed.components, dataset.components);
        assert_eq!(reparsed.phases, dataset.phases);
        assert_eq!(reparsed.grids.len(), dataset.grids.len());
    }

    #[test]
    fn classified_values_serialize_deterministically_and_round_trip() {
        let dataset = parse_str(include_str!("../fixtures/classified-states.tct")).unwrap();
        let text = serialize_tct(&dataset, &TctSerializeOptions::default()).unwrap();
        assert!(text.contains("CO:3000"));
        assert!(text.contains("\tNE"));
        let reparsed = parse_str(&text).unwrap();
        assert_eq!(
            reparsed.grids[0].fields()[0].values,
            dataset.grids[0].fields()[0].values
        );
        assert_eq!(
            reparsed.grids[0].fields()[1].values,
            dataset.grids[0].fields()[1].values
        );
    }

    #[test]
    fn missing_values_use_configured_token() {
        let mut dataset = parse_str(include_str!("../fixtures/minimal-regular.tct")).unwrap();
        match &mut dataset.grids[0] {
            TabulatedGrid::Regular(grid) => {
                grid.fields[0].values[0] = crate::TabulatedValue::missing()
            }
            TabulatedGrid::Irregular(grid) => {
                grid.fields[0].values[0] = crate::TabulatedValue::missing()
            }
        }
        let text = serialize_tct(
            &dataset,
            &TctSerializeOptions {
                missing_token: "MISSING".into(),
                ..TctSerializeOptions::default()
            },
        )
        .unwrap();
        assert!(text.contains("MISSING"));
    }
}
