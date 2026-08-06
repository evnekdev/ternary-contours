//! Canonical, deterministic descriptions of phase-labelled stable contours.
//!
//! These comparison records intentionally retain continuous junction identity
//! and phase incidence while discarding dense vector IDs.  They are diagnostic
//! data only and never participate in numerical cache keys.

use super::{
    StableContourJunctionKind, StableContourPathGeometryState, StableContourSet, StablePhaseId,
    StableTopologyComparisonMode,
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StableContourGeometrySignature {
    Ignored,
    Quantized([i64; 3]),
    Exact([u64; 3]),
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StableContourJunctionSignature {
    pub kind: StableContourJunctionKind,
    pub phases: Vec<u32>,
    pub branch: Option<usize>,
    pub invariant: Option<usize>,
    pub point: StableContourGeometrySignature,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StableContourHalfEdgeSignature {
    pub junction: StableContourJunctionSignature,
    pub phase: u32,
    pub at_start: bool,
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StableContourPathSignature {
    pub phase: StablePhaseId,
    pub closed: bool,
    pub start: Option<StableContourJunctionSignature>,
    pub end: Option<StableContourJunctionSignature>,
    pub geometry_state: StableContourPathGeometryState,
    pub geometry: Vec<StableContourGeometrySignature>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StableContourLevelSignature {
    pub level: u64,
    pub junctions: Vec<StableContourJunctionSignature>,
    pub half_edges: Vec<StableContourHalfEdgeSignature>,
    pub paths: Vec<StableContourPathSignature>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StableContourSignature {
    pub mode: StableTopologyComparisonMode,
    pub quantity: super::StableContourQuantity,
    pub levels: Vec<StableContourLevelSignature>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StableContourComparison {
    pub mode: StableTopologyComparisonMode,
    pub equal: bool,
    pub differences: Vec<String>,
}

const QUANTUM: f64 = 1.0e-3;

fn geometry(point: [f64; 3], mode: StableTopologyComparisonMode) -> StableContourGeometrySignature {
    match mode {
        StableTopologyComparisonMode::TopologyOnly => StableContourGeometrySignature::Ignored,
        StableTopologyComparisonMode::ToleranceAwareGeometry => {
            StableContourGeometrySignature::Quantized(point.map(|value| {
                (value / QUANTUM)
                    .round()
                    .clamp(i64::MIN as f64, i64::MAX as f64) as i64
            }))
        }
        StableTopologyComparisonMode::ExactDiagnostic => {
            StableContourGeometrySignature::Exact(point.map(f64::to_bits))
        }
    }
}

fn junction_signature(
    level: &super::StableContourLevel,
    id: super::StableJunctionId,
    mode: StableTopologyComparisonMode,
) -> Option<StableContourJunctionSignature> {
    let junction = level.junctions.get(id.0)?;
    let mut phases = junction
        .phases
        .iter()
        .map(|phase| phase.0)
        .collect::<Vec<_>>();
    phases.sort_unstable();
    phases.dedup();
    Some(StableContourJunctionSignature {
        kind: junction.kind,
        phases,
        branch: junction.branch.map(|id| id.0),
        invariant: junction.invariant.map(|id| id.0),
        point: geometry(junction.point.as_array(), mode),
    })
}

/// Build a deterministic comparison signature for all requested contour levels.
pub fn stable_contour_signature(
    contours: &StableContourSet,
    mode: StableTopologyComparisonMode,
) -> StableContourSignature {
    let mut levels = contours
        .levels
        .iter()
        .map(|level| {
            let mut junctions = level
                .junctions
                .iter()
                .filter_map(|junction| junction_signature(level, junction.id, mode))
                .collect::<Vec<_>>();
            junctions.sort();
            let mut half_edges = level
                .half_edges
                .iter()
                .filter_map(|edge| {
                    junction_signature(level, edge.junction, mode).map(|junction| {
                        StableContourHalfEdgeSignature {
                            junction,
                            phase: edge.phase.0,
                            at_start: edge.at_start,
                        }
                    })
                })
                .collect::<Vec<_>>();
            half_edges.sort();
            let mut paths = level
                .paths
                .iter()
                .map(|path| StableContourPathSignature {
                    phase: path.phase,
                    closed: path.closed,
                    start: path
                        .start_junction
                        .and_then(|id| junction_signature(level, id, mode)),
                    end: path
                        .end_junction
                        .and_then(|id| junction_signature(level, id, mode)),
                    geometry_state: match mode {
                        StableTopologyComparisonMode::TopologyOnly => {
                            StableContourPathGeometryState::Raw
                        }
                        StableTopologyComparisonMode::ToleranceAwareGeometry
                        | StableTopologyComparisonMode::ExactDiagnostic => path.geometry_state,
                    },
                    geometry: match mode {
                        StableTopologyComparisonMode::TopologyOnly => Vec::new(),
                        StableTopologyComparisonMode::ToleranceAwareGeometry
                        | StableTopologyComparisonMode::ExactDiagnostic => path
                            .points
                            .iter()
                            .map(|point| geometry(point.as_array(), mode))
                            .collect(),
                    },
                })
                .collect::<Vec<_>>();
            paths.sort();
            StableContourLevelSignature {
                level: match mode {
                    StableTopologyComparisonMode::TopologyOnly => 0,
                    StableTopologyComparisonMode::ToleranceAwareGeometry => {
                        (level.value / QUANTUM).round().to_bits()
                    }
                    StableTopologyComparisonMode::ExactDiagnostic => level.value.to_bits(),
                },
                junctions,
                half_edges,
                paths,
            }
        })
        .collect::<Vec<_>>();
    levels.sort_by_key(|level| level.level);
    StableContourSignature {
        mode,
        quantity: contours.quantity,
        levels,
    }
}

pub fn compare_stable_contours(
    left: &StableContourSignature,
    right: &StableContourSignature,
) -> StableContourComparison {
    let mut differences = Vec::new();
    if left.mode != right.mode {
        differences.push(format!(
            "comparison modes differ: {:?} versus {:?}",
            left.mode, right.mode
        ));
    }
    if left.quantity != right.quantity {
        differences.push(format!(
            "contour quantities differ: {:?} versus {:?}",
            left.quantity, right.quantity
        ));
    }
    if left.levels != right.levels {
        differences.push(format!(
            "contour topology differs: left={:?}; right={:?}",
            left.levels, right.levels
        ));
    }
    StableContourComparison {
        mode: left.mode,
        equal: differences.is_empty(),
        differences,
    }
}
