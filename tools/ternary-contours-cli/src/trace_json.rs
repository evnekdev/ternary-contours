//! JSON Lines persistence and structural analysis for the optional numerical trace.
//!
//! This module is deliberately owned by the command-line tool.  The numerical
//! core only owns typed deterministic events and never performs file I/O.

use std::{
    fs::File,
    io::{BufRead, BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use ternary_contours::{
    NumericalTraceConfig, NumericalTraceEvent, NumericalTraceEventKind, NumericalTraceSink,
};

/// Status of an observation output stream.  A stream failure never changes the
/// associated numerical calculation result.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TraceOutputStatus {
    pub path: PathBuf,
    pub events_written: usize,
    pub truncated: bool,
    pub first_error: Option<String>,
}

/// JSON Lines sink for a single calculation request.
pub struct JsonLinesTraceSink {
    config: NumericalTraceConfig,
    writer: BufWriter<File>,
    status: TraceOutputStatus,
}

impl JsonLinesTraceSink {
    pub fn create(
        path: impl AsRef<Path>,
        config: NumericalTraceConfig,
    ) -> Result<Self, std::io::Error> {
        let path = path.as_ref().to_path_buf();
        let writer = BufWriter::new(File::create(&path)?);
        Ok(Self {
            config,
            writer,
            status: TraceOutputStatus {
                path,
                ..TraceOutputStatus::default()
            },
        })
    }

    /// Flush the stream and return observation status separately from any
    /// numerical result returned earlier.
    pub fn finish(mut self) -> TraceOutputStatus {
        if self.status.first_error.is_none()
            && let Err(error) = self.writer.flush()
        {
            self.status.first_error = Some(error.to_string());
        }
        self.status
    }
}

impl NumericalTraceSink for JsonLinesTraceSink {
    fn config(&self) -> &NumericalTraceConfig {
        &self.config
    }

    fn record(&mut self, event: NumericalTraceEvent) {
        if self.status.first_error.is_some() {
            return;
        }
        if matches!(
            event.payload.kind(),
            NumericalTraceEventKind::TraceTruncated
        ) {
            self.status.truncated = true;
        }
        let result = serde_json::to_writer(&mut self.writer, &event)
            .and_then(|()| self.writer.write_all(b"\n").map_err(serde_json::Error::io));
        match result {
            Ok(()) => self.status.events_written += 1,
            Err(error) => self.status.first_error = Some(error.to_string()),
        }
    }
}

/// Compact structural report produced by `analyze-trace`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TraceAnalysis {
    pub schema_versions: Vec<u32>,
    pub event_count: usize,
    pub binary_boundaries_started: usize,
    pub binary_invariants: usize,
    /// Sampled binary phase transitions unsupported by finite source coverage.
    pub unavailable_binary_transitions: usize,
    pub interior_invariants: usize,
    pub univariants_completed: usize,
    pub contours_completed: usize,
    pub trace_truncated: bool,
    pub warnings: Vec<String>,
}

impl TraceAnalysis {
    pub fn is_consistent(&self) -> bool {
        self.warnings.is_empty() && !self.trace_truncated
    }
}

/// Analyze JSON Lines structure without claiming mathematical correctness.
pub fn analyze_trace(path: impl AsRef<Path>) -> Result<TraceAnalysis, String> {
    let input = File::open(path.as_ref()).map_err(|error| error.to_string())?;
    let mut analysis = TraceAnalysis::default();
    let mut previous_sequence = None;
    let mut started = false;
    let mut terminal = false;
    for (line_index, line) in BufReader::new(input).lines().enumerate() {
        let line = line.map_err(|error| error.to_string())?;
        if line.trim().is_empty() {
            continue;
        }
        let event: NumericalTraceEvent = serde_json::from_str(&line)
            .map_err(|error| format!("line {}: {error}", line_index + 1))?;
        if let Some(previous) = previous_sequence
            && event.sequence <= previous
        {
            analysis.warnings.push(format!(
                "possible inconsistency: sequence {} follows {}",
                event.sequence, previous
            ));
        }
        previous_sequence = Some(event.sequence);
        if !analysis.schema_versions.contains(&event.schema_version) {
            analysis.schema_versions.push(event.schema_version);
        }
        analysis.event_count += 1;
        match event.payload.kind() {
            NumericalTraceEventKind::RunStarted => {
                if started {
                    analysis
                        .warnings
                        .push("possible inconsistency: multiple RunStarted events".into());
                }
                started = true;
            }
            NumericalTraceEventKind::RunCompleted | NumericalTraceEventKind::RunFailed => {
                if terminal {
                    analysis
                        .warnings
                        .push("possible inconsistency: multiple terminal run events".into());
                }
                terminal = true;
            }
            NumericalTraceEventKind::TraceTruncated => analysis.trace_truncated = true,
            NumericalTraceEventKind::BinaryBoundaryStarted => {
                analysis.binary_boundaries_started += 1
            }
            NumericalTraceEventKind::BinaryInvariantEmitted => analysis.binary_invariants += 1,
            NumericalTraceEventKind::BinaryTransitionUnavailable => {
                analysis.unavailable_binary_transitions += 1
            }
            NumericalTraceEventKind::InteriorInvariantAccepted => analysis.interior_invariants += 1,
            NumericalTraceEventKind::UnivariantTraceCompleted => {
                analysis.univariants_completed += 1
            }
            NumericalTraceEventKind::ContourPathCompleted => analysis.contours_completed += 1,
            _ => {}
        }
    }
    if !started {
        analysis
            .warnings
            .push("possible inconsistency: RunStarted is absent".into());
    }
    if !terminal {
        analysis
            .warnings
            .push("possible inconsistency: terminal RunCompleted/RunFailed is absent".into());
    }
    if analysis.trace_truncated {
        analysis
            .warnings
            .push("trace incomplete: TraceTruncated was emitted".into());
    }
    analysis.schema_versions.sort_unstable();
    Ok(analysis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use ternary_contours::{
        NumericalTraceLevel, NumericalTraceSession, NumericalTraceStage, TraceDecision, decision,
    };

    #[test]
    fn json_lines_are_deterministic_and_structurally_analyzable() {
        let path = std::env::temp_dir().join(format!(
            "ternary-contours-trace-{}-{}.jsonl",
            std::process::id(),
            "deterministic"
        ));
        let mut sink = JsonLinesTraceSink::create(
            &path,
            NumericalTraceConfig {
                level: NumericalTraceLevel::Summary,
                maximum_events: 10,
                ..NumericalTraceConfig::default()
            },
        )
        .unwrap();
        {
            let mut trace = NumericalTraceSession::new(&mut sink);
            trace.emit(
                NumericalTraceLevel::Summary,
                NumericalTraceStage::Run,
                decision(
                    NumericalTraceEventKind::RunStarted,
                    TraceDecision::default(),
                ),
            );
            trace.emit(
                NumericalTraceLevel::Summary,
                NumericalTraceStage::Run,
                decision(
                    NumericalTraceEventKind::RunCompleted,
                    TraceDecision::default(),
                ),
            );
        }
        assert!(sink.finish().first_error.is_none());
        let analysis = analyze_trace(&path).unwrap();
        assert_eq!(analysis.event_count, 2);
        assert!(analysis.is_consistent());
        fs::remove_file(path).unwrap();
    }
}
