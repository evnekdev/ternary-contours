//! Headless regular/irregular table template generation.

use std::collections::BTreeSet;

use crate::{NumericFormat, regular_compositions};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IrregularTemplateStyle {
    FullTct,
    GridSection,
    TsvHeader,
}

pub fn parse_components(text: &str) -> Result<[String; 3], String> {
    let values = text
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    let components: [String; 3] = values.try_into().map_err(|values: Vec<String>| {
        format!(
            "expected exactly three comma-separated component names; found {}",
            values.len()
        )
    })?;
    if components
        .iter()
        .enumerate()
        .any(|(index, value)| value.is_empty() || components[..index].contains(value))
    {
        return Err("component names must be non-blank and unique".into());
    }
    Ok(components)
}

pub fn parse_field_specs(text: &str) -> Result<Vec<(String, String)>, String> {
    let fields = text
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            let (phase, property) = value
                .split_once('.')
                .ok_or_else(|| format!("field '{value}' must use phase.property notation"))?;
            if phase.is_empty() || property.is_empty() || property.contains('.') {
                return Err(format!("field '{value}' must use phase.property notation"));
            }
            Ok((phase.to_owned(), property.to_owned()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if fields.is_empty() {
        return Err("at least one phase.property field is required".into());
    }
    let mut seen = BTreeSet::new();
    if fields.iter().any(|field| !seen.insert(field.clone())) {
        return Err("field declarations must be unique".into());
    }
    Ok(fields)
}

pub fn regular_template_tct(
    subdivisions: usize,
    components: [String; 3],
    fields: &[(String, String)],
    numeric: NumericFormat,
) -> Result<String, String> {
    let points = regular_compositions(subdivisions)?;
    let (phases, properties) = declarations(fields);
    let mut output = document_prefix(&components, &phases, &properties);
    output.push_str("\n[grid regular_template]\ntype = regular\n");
    output.push_str(&format!(
        "subdivisions = {subdivisions}\norder = canonical\ncomposition_columns = none\n"
    ));
    output.push_str("properties = ");
    output.push_str(&properties.join(" "));
    output.push_str("\ncolumns:\n");
    output.push_str(&field_headers(fields).join("\t"));
    output.push_str("\ndata:\n");
    for _ in points {
        output.push_str(&vec!["NA"; fields.len()].join("\t"));
        output.push('\n');
    }
    output.push_str("[/grid]\n");
    let _ = numeric;
    Ok(output)
}

pub fn irregular_template(
    components: [String; 3],
    fields: &[(String, String)],
    style: IrregularTemplateStyle,
) -> String {
    let header = {
        let mut columns = components.to_vec();
        columns.extend(field_headers(fields));
        columns.join("\t")
    };
    match style {
        IrregularTemplateStyle::TsvHeader => header + "\n",
        IrregularTemplateStyle::GridSection => format!(
            "[grid irregular_template]\ntype = irregular\ncomposition_columns = authoritative\nproperties = {}\ncolumns:\n{header}\ndata:\n[/grid]\n",
            properties_for(fields).join(" ")
        ),
        IrregularTemplateStyle::FullTct => {
            let (phases, properties) = declarations(fields);
            let mut output = document_prefix(&components, &phases, &properties);
            output.push('\n');
            output.push_str(&irregular_template(
                components,
                fields,
                IrregularTemplateStyle::GridSection,
            ));
            output
        }
    }
}

fn declarations(fields: &[(String, String)]) -> (Vec<String>, Vec<String>) {
    let mut phases = fields
        .iter()
        .map(|(phase, _)| phase.clone())
        .collect::<Vec<_>>();
    phases.sort();
    phases.dedup();
    let mut properties = properties_for(fields);
    if !properties.iter().any(|property| property == "T") {
        properties.push("T".into());
        properties.sort();
    }
    (phases, properties)
}

fn properties_for(fields: &[(String, String)]) -> Vec<String> {
    let mut properties = fields
        .iter()
        .map(|(_, property)| property.clone())
        .collect::<Vec<_>>();
    properties.sort();
    properties.dedup();
    properties
}

fn document_prefix(components: &[String; 3], phases: &[String], properties: &[String]) -> String {
    let mut output = String::from(
        "TCT 1.0\n\ntitle = \"Ternary contours data-entry template\"\ncomposition_units = fraction\ndefault_missing = NA\n\n[components]\n",
    );
    for component in components {
        output.push_str(component);
        output.push('\n');
    }
    output.push_str("[/components]\n\n[phases]\n");
    for (index, phase) in phases.iter().enumerate() {
        output.push_str(&format!("{phase} = {}\n", index + 1));
    }
    output.push_str("[/phases]\n\n[properties]\n");
    for property in properties {
        output.push_str(property);
        output.push_str(if property == "T" {
            " required K\n"
        } else {
            " optional 1\n"
        });
    }
    output.push_str("[/properties]\n");
    output
}

fn field_headers(fields: &[(String, String)]) -> Vec<String> {
    fields
        .iter()
        .map(|(phase, property)| format!("{phase}.{property}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_and_field_specs_are_deterministic() {
        let fields = parse_field_specs("alpha.T,beta.T").unwrap();
        let regular = regular_template_tct(
            2,
            parse_components("A,B,C").unwrap(),
            &fields,
            NumericFormat::default(),
        )
        .unwrap();
        assert!(regular.contains("subdivisions = 2"));
        assert_eq!(regular.lines().filter(|line| *line == "NA\tNA").count(), 6);
        assert_eq!(
            irregular_template(
                parse_components("A,B,C").unwrap(),
                &fields,
                IrregularTemplateStyle::TsvHeader
            ),
            "A\tB\tC\talpha.T\tbeta.T\n"
        );
    }
}
