//! Rectangular tab-separated table parsing shared by TCT and clipboard workflows.
//!
//! The TCT section parser owns declarations and section boundaries. This module
//! only understands rectangular tabular data and retains source coordinates so a
//! caller can report either a file line or a clipboard row.

use std::fmt;

/// One 1-based location in a tabular source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableLocation {
    pub row: usize,
    pub column: usize,
}

impl fmt::Display for TableLocation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "row {}, column {}", self.row, self.column)
    }
}

/// One unmodified cell with a useful source location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedCell {
    pub text: String,
    pub location: TableLocation,
}

/// A non-header row. source_row is 1-based within the original table text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedRow {
    pub source_row: usize,
    pub cells: Vec<ParsedCell>,
}

/// A rectangular TSV table with an optional header row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParsedTable {
    pub headers: Option<Vec<String>>,
    pub rows: Vec<ParsedRow>,
}

/// The header treatment for a table source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeaderMode {
    /// Treat the first non-empty row as headers.
    Present,
    /// Preserve every row as data.
    Absent,
    /// Infer headers when the first row has at least one non-numeric cell.
    Detect,
}

/// A source-aware rectangular-table failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TableError {
    pub location: Option<TableLocation>,
    pub message: String,
}

impl fmt::Display for TableError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.location {
            Some(location) => write!(formatter, "{location}: {}", self.message),
            None => formatter.write_str(&self.message),
        }
    }
}

impl std::error::Error for TableError {}

/// Split one TSV row while preserving literal blank cells and source columns.
/// TCT data sections call this too, so clipboard and file data share the same
/// low-level cell boundary behaviour.
pub fn parse_tsv_row(source_row: usize, line: &str) -> ParsedRow {
    ParsedRow {
        source_row,
        cells: line
            .split('\t')
            .enumerate()
            .map(|(column, text)| ParsedCell {
                text: text.trim().to_owned(),
                location: TableLocation {
                    row: source_row,
                    column: column + 1,
                },
            })
            .collect(),
    }
}
impl ParsedTable {
    /// Parse a literal TSV range. Empty trailing cells are retained; one empty
    /// line introduced solely by a final line ending is ignored, while blank
    /// rows inside the range remain visible and can be rejected as ragged data.
    pub fn parse_tsv(input: &str, header_mode: HeaderMode) -> Result<Self, TableError> {
        let mut lines = input.split('\n').collect::<Vec<_>>();
        if input.ends_with('\n') {
            lines.pop();
        }
        let raw_rows = lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| parse_tsv_row(index + 1, line.strip_suffix('\r').unwrap_or(line)))
            .collect::<Vec<_>>();
        Self::from_rows(raw_rows, header_mode)
    }

    /// Construct a table from source-labelled rows. TCT uses this to reuse the
    /// same rectangular-width and header validation as clipboard data.
    pub fn from_rows(
        mut rows: Vec<ParsedRow>,
        header_mode: HeaderMode,
    ) -> Result<Self, TableError> {
        if rows.is_empty() {
            return Err(TableError {
                location: None,
                message: "table contains no rows".into(),
            });
        }
        let first = rows.first().expect("checked non-empty rows");
        if first.cells.is_empty() {
            return Err(TableError {
                location: Some(TableLocation {
                    row: first.source_row,
                    column: 1,
                }),
                message: "table row contains no cells".into(),
            });
        }
        let has_header = match header_mode {
            HeaderMode::Present => true,
            HeaderMode::Absent => false,
            HeaderMode::Detect => first
                .cells
                .iter()
                .any(|cell| cell.text.parse::<f64>().is_err()),
        };
        let headers = has_header.then(|| {
            rows.remove(0)
                .cells
                .into_iter()
                .map(|cell| cell.text)
                .collect::<Vec<_>>()
        });
        let width = headers
            .as_ref()
            .map(Vec::len)
            .or_else(|| rows.first().map(|row| row.cells.len()))
            .unwrap_or(0);
        if width == 0 {
            return Err(TableError {
                location: None,
                message: "table must contain at least one column".into(),
            });
        }
        if let Some(headers) = &headers {
            if headers.iter().any(String::is_empty) {
                return Err(TableError {
                    location: Some(TableLocation { row: 1, column: 1 }),
                    message: "header cells must not be blank".into(),
                });
            }
            for (index, header) in headers.iter().enumerate() {
                if headers[..index].iter().any(|previous| previous == header) {
                    return Err(TableError {
                        location: Some(TableLocation {
                            row: 1,
                            column: index + 1,
                        }),
                        message: format!("duplicate header '{header}'"),
                    });
                }
            }
        }
        for row in &rows {
            if row.cells.len() != width {
                return Err(TableError {
                    location: Some(TableLocation {
                        row: row.source_row,
                        column: row.cells.len() + 1,
                    }),
                    message: format!(
                        "wrong row width: expected {width} tab-separated cells, found {}",
                        row.cells.len()
                    ),
                });
            }
        }
        Ok(Self { headers, rows })
    }

    pub fn width(&self) -> usize {
        self.headers
            .as_ref()
            .map(Vec::len)
            .or_else(|| self.rows.first().map(|row| row.cells.len()))
            .unwrap_or(0)
    }

    pub fn height(&self) -> usize {
        self.rows.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retains_blank_cells_and_source_locations() {
        let table = ParsedTable::parse_tsv(
            "A	B
1	
",
            HeaderMode::Present,
        )
        .unwrap();
        assert_eq!(table.headers, Some(vec!["A".into(), "B".into()]));
        assert_eq!(table.rows[0].cells[1].text, "");
        assert_eq!(
            table.rows[0].cells[1].location,
            TableLocation { row: 2, column: 2 }
        );
    }

    #[test]
    fn trailing_newline_is_ignored_but_internal_blank_rows_are_not() {
        let table = ParsedTable::parse_tsv("1\t2\n3\t4\n", HeaderMode::Absent).unwrap();
        assert_eq!(table.height(), 2);
        let error = ParsedTable::parse_tsv("1\t2\n\n3\t4", HeaderMode::Absent).unwrap_err();
        assert!(error.message.contains("wrong row width"));
    }

    #[test]
    fn detects_headers_without_assuming_tct_semantics() {
        let table = ParsedTable::parse_tsv(
            "A	B
1	2",
            HeaderMode::Detect,
        )
        .unwrap();
        assert_eq!(table.headers.unwrap(), vec!["A", "B"]);
        let data = ParsedTable::parse_tsv(
            "1	2
3	4",
            HeaderMode::Detect,
        )
        .unwrap();
        assert!(data.headers.is_none());
        assert_eq!(data.height(), 2);
    }
}
