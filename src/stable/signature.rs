//! Canonical, deterministic descriptions of a stable-boundary network.
//!
//! These signatures intentionally use physical graph identity rather than the
//! dense node and path IDs assigned during one calculation. They are shared
//! by the numerical audit, regression tests, and regularization checks.

use super::{
    StableBoundaryError, StableBoundaryNetwork, StableInvariantNode, StablePathGeometryState,
    StablePhasePair,
};

/// Comparison granularity for a StableTopologySignature.
///
/// ToleranceAwareGeometry quantizes compositions and temperatures to 0.001.
/// This is deliberately a comparison representation only: it never changes a
/// calculated node or path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StableTopologyComparisonMode {
    /// Phase sets, incidence, and connectivity only.
    TopologyOnly,
    /// Topology plus geometry quantized to 0.001 in composition and scalar
    /// units.
    ToleranceAwareGeometry,
    /// Full IEEE-754 diagnostic identity for one deterministic run.
    ExactDiagnostic,
}

/// Whether a node is located on an outer binary edge or in the simplex.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StableNodeKindSignature {
    Binary,
    Interior,
}

/// Geometry portion of a canonical node description.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum StableNodeGeometrySignature {
    /// Geometry is deliberately excluded by TopologyOnly comparison.
    Ignored,
    /// Composition and temperature rounded to the documented audit quantum.
    Quantized {
        composition: [i64; 3],
        temperature: i64,
    },
    /// Exact IEEE-754 bits, intended for repeatability diagnostics.
    Exact {
        composition: [u64; 3],
        temperature: u64,
    },
}

/// Canonical description of an invariant node without a transient node ID.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StableNodeSignature {
    pub kind: StableNodeKindSignature,
    pub phase_ids: Vec<u32>,
    pub incidence_degree: usize,
    pub incident_phase_pairs: Vec<StablePhasePair>,
    pub geometry: StableNodeGeometrySignature,
}

/// Canonically undirected stable univariant edge.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StableEdgeSignature {
    pub phase_pair: StablePhasePair,
    pub endpoints: [StableNodeSignature; 2],
    /// Present only in geometry-aware modes. Topology comparisons deliberately
    /// ignore raw/regularized presentation so raw and regularized variants can
    /// prove they share the same graph.
    pub geometry_state: Option<StablePathGeometryState>,
}

/// Canonical description of a diagnostic-only domain-truncated branch.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StableTruncatedBranchSignature {
    pub phase_pair: StablePhasePair,
    pub start: StableNodeSignature,
}

/// Stable graph identity suitable for repeatability and convergence checks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StableTopologySignature {
    pub mode: StableTopologyComparisonMode,
    pub nodes: Vec<StableNodeSignature>,
    pub edges: Vec<StableEdgeSignature>,
    pub truncated_branches: Vec<StableTruncatedBranchSignature>,
}

/// Human-readable outcome of comparing two canonical signatures.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StableTopologyComparison {
    pub mode: StableTopologyComparisonMode,
    pub equal: bool,
    pub differences: Vec<String>,
}

const AUDIT_GEOMETRY_QUANTUM: f64 = 1.0e-3;

fn quantize(value: f64) -> i64 {
    // Stable-boundary values are finite by construction. Saturation gives a
    // deterministic diagnostic even for a malformed hand-built network.
    let scaled = (value / AUDIT_GEOMETRY_QUANTUM).round();
    scaled.clamp(i64::MIN as f64, i64::MAX as f64) as i64
}

fn geometry_signature(
    node: &StableInvariantNode,
    mode: StableTopologyComparisonMode,
) -> StableNodeGeometrySignature {
    let point = node.point().as_array();
    match mode {
        StableTopologyComparisonMode::TopologyOnly => StableNodeGeometrySignature::Ignored,
        StableTopologyComparisonMode::ToleranceAwareGeometry => {
            StableNodeGeometrySignature::Quantized {
                composition: [quantize(point[0]), quantize(point[1]), quantize(point[2])],
                temperature: quantize(node.temperature()),
            }
        }
        StableTopologyComparisonMode::ExactDiagnostic => StableNodeGeometrySignature::Exact {
            composition: [point[0].to_bits(), point[1].to_bits(), point[2].to_bits()],
            temperature: node.temperature().to_bits(),
        },
    }
}

fn node_signature(
    network: &StableBoundaryNetwork,
    index: usize,
    mode: StableTopologyComparisonMode,
) -> Result<StableNodeSignature, StableBoundaryError> {
    let node = network.nodes.get(index).ok_or_else(|| {
        StableBoundaryError::MalformedGraphConnectivity {
            message: format!("signature references missing invariant node {index}"),
        }
    })?;
    let mut phase_ids = node
        .phases()
        .iter()
        .map(|phase| phase.0)
        .collect::<Vec<_>>();
    phase_ids.sort_unstable();
    phase_ids.dedup();
    let incidents = network.incident_univariants(node.id())?;
    let mut incident_phase_pairs = incidents
        .iter()
        .map(|path_id| {
            network
                .univariants
                .get(path_id.0)
                .map(|path| path.phases)
                .ok_or_else(|| StableBoundaryError::MalformedGraphConnectivity {
                    message: format!("signature references missing univariant {}", path_id.0),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    incident_phase_pairs.sort_unstable();
    incident_phase_pairs.dedup();
    Ok(StableNodeSignature {
        kind: if matches!(node, StableInvariantNode::Binary(_)) {
            StableNodeKindSignature::Binary
        } else {
            StableNodeKindSignature::Interior
        },
        phase_ids,
        incidence_degree: incidents.len(),
        incident_phase_pairs,
        geometry: geometry_signature(node, mode),
    })
}

/// Build a deterministic graph signature from one accepted stable network.
pub fn stable_topology_signature(
    network: &StableBoundaryNetwork,
    mode: StableTopologyComparisonMode,
) -> Result<StableTopologySignature, StableBoundaryError> {
    let node_by_id = (0..network.nodes.len())
        .map(|index| node_signature(network, index, mode))
        .collect::<Result<Vec<_>, _>>()?;
    let mut nodes = node_by_id.clone();
    nodes.sort();

    let mut edges = network
        .univariants
        .iter()
        .map(|path| {
            let start = node_by_id.get(path.start.0).cloned().ok_or_else(|| {
                StableBoundaryError::MalformedGraphConnectivity {
                    message: format!("path {} starts at missing node {}", path.id.0, path.start.0),
                }
            })?;
            let end = node_by_id.get(path.end.0).cloned().ok_or_else(|| {
                StableBoundaryError::MalformedGraphConnectivity {
                    message: format!("path {} ends at missing node {}", path.id.0, path.end.0),
                }
            })?;
            let mut endpoints = [start, end];
            endpoints.sort();
            Ok(StableEdgeSignature {
                phase_pair: path.phases,
                endpoints,
                geometry_state: match mode {
                    StableTopologyComparisonMode::TopologyOnly => None,
                    StableTopologyComparisonMode::ToleranceAwareGeometry
                    | StableTopologyComparisonMode::ExactDiagnostic => {
                        network.path_geometry_state(path.id)
                    }
                },
            })
        })
        .collect::<Result<Vec<_>, StableBoundaryError>>()?;
    edges.sort();

    let mut truncated_branches = network
        .truncated_univariants
        .iter()
        .map(|branch| {
            node_by_id
                .get(branch.start.0)
                .cloned()
                .map(|start| StableTruncatedBranchSignature {
                    phase_pair: branch.phases,
                    start,
                })
                .ok_or_else(|| StableBoundaryError::MalformedGraphConnectivity {
                    message: format!(
                        "truncated branch starts at missing invariant node {}",
                        branch.start.0
                    ),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    truncated_branches.sort();

    Ok(StableTopologySignature {
        mode,
        nodes,
        edges,
        truncated_branches,
    })
}

impl StableTopologySignature {
    /// Stable diagnostic text used as the input to audit hashes.
    pub fn canonical_text(&self) -> String {
        format!(
            "{:?}|{:?}|{:?}",
            self.nodes, self.edges, self.truncated_branches
        )
    }
}

/// Compare two network signatures and retain a concise readable difference.
pub fn compare_stable_topology(
    left: &StableTopologySignature,
    right: &StableTopologySignature,
) -> StableTopologyComparison {
    let mut differences = Vec::new();
    if left.mode != right.mode {
        differences.push(format!(
            "comparison modes differ: {:?} versus {:?}",
            left.mode, right.mode
        ));
    }
    if left.nodes != right.nodes {
        differences.push(format!(
            "invariant nodes differ: left={:?}; right={:?}",
            left.nodes, right.nodes
        ));
    }
    if left.edges != right.edges {
        differences.push(format!(
            "univariant edges differ: left={:?}; right={:?}",
            left.edges, right.edges
        ));
    }
    if left.truncated_branches != right.truncated_branches {
        differences.push(format!(
            "truncated branches differ: left={:?}; right={:?}",
            left.truncated_branches, right.truncated_branches
        ));
    }
    StableTopologyComparison {
        mode: left.mode,
        equal: differences.is_empty(),
        differences,
    }
}

/// Assert that two stable networks have the same physical topology.
///
/// This deliberately uses TopologyOnly so raw and regularized representations
/// may differ in their path samples and geometry state without changing their
/// invariant graph.
pub fn assert_same_stable_topology(
    left: &StableBoundaryNetwork,
    right: &StableBoundaryNetwork,
) -> Result<(), String> {
    let left = stable_topology_signature(left, StableTopologyComparisonMode::TopologyOnly)
        .map_err(|error| error.to_string())?;
    let right = stable_topology_signature(right, StableTopologyComparisonMode::TopologyOnly)
        .map_err(|error| error.to_string())?;
    let comparison = compare_stable_topology(&left, &right);
    if comparison.equal {
        Ok(())
    } else {
        Err(comparison.differences.join(
            "
",
        ))
    }
}
