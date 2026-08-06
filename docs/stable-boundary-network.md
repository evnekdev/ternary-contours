# Stable invariant and univariant boundary networks

## Scope

This numerical API constructs the boundary-connected, level-free graph of the
stable upper envelope. It is independent of rendering and independent of the
level-specific stable-isotherm extractor. The implementation does not redesign
stable polygons or source interpolation: it consumes the authoritative prepared
phase ensemble and its cached affine polygons on the common regular sampling
grid.

The current result includes:

- stable binary invariants on canonical AB, BC, and CA boundaries;
- stable interior invariants, including higher-order ties;
- canonical phase-pair paths connecting those nodes;
- node-to-path incidence and dense deterministic identifiers;
- raw and optionally regularized path coordinates;
- binary-discovery, topology, traversal, and regularization diagnostics.

An equality below another phase is metastable and is not part of this graph.
Isolated closed univariant loops without a binary or invariant seed are deferred.

## Partial phase domains

A `StableScalarSource::evaluator` returns `StablePhaseEvaluation`:

```text
Defined { value }
Undefined { reason }
```

Undefined reasons distinguish an outside phase domain, missing source support,
non-finite evaluator output, and user-defined exclusions. An undefined phase is
omitted from the local stable maximum. It is never assigned a sentinel value and
is never extrapolated. Construction returns `NoPhaseDefined` when the union of
phase domains leaves a sampled or queried point uncovered.

This policy also applies during regularization. Both members of a projected
phase pair must remain defined. Undefined competitors are omitted consistently
from the upper envelope.

A phase-pair equality that reaches an unavailable source-domain boundary before
a second invariant is retained as a typed `StableTruncatedUnivariantPath` with
`UnivariantTermination::ReachedSourceDomainBoundary`. It is diagnostic-only:
it is neither fabricated into an invariant connection nor counted as a complete
univariant. Only the pending ends carried by that traversed branch are consumed;
other pending ends for the same phase pair remain independently traceable.

## Canonical binary discovery

Boundary parameters increase as follows:

```text
AB: (1-t, t, 0)
BC: (0, 1-t, t)
CA: (t, 0, 1-t)
```

Discovery evaluates a deterministic dyadic parameter grid derived from
`binary_initial_subdivisions` and `binary_maximum_depth`. Full phase sweeps and
individual phase results are cached by exact parameter bits. Ordered stable
regions identify candidate phase transitions. Pair-only evaluations then find
an overlapping defined bracket and use safeguarded secant/bisection refinement.
The final root is accepted only after a new full sweep confirms both phases on
the stable envelope. Nearby coincident transitions merge into one higher-order
node with sorted phase IDs.

The binary root is authoritative. A sampling-cell interval is used only to
find local compatible refined-root candidates. Exactly one candidate must
remain; zero candidates produce `NoMatchingBinaryNode` and several candidates
produce `AmbiguousBinaryEndpointMatch` with the candidate node IDs. The
implementation never resolves an ambiguity by a nearest-node ID tie-break. The
selected endpoint is then replaced by the canonical refined binary coordinate.

## Dense sampling-grid topology

`RegularSamplingTopology` is the backend-neutral topology cache for the common
regular grid. It deterministically enumerates all three lattice-direction edge
families and stores:

- canonical dense edge IDs and endpoint vertex IDs;
- up to two incident triangle IDs per edge;
- the three edge IDs of every triangle;
- triangle-local reversal relative to each canonical edge;
- opposite-triangle lookup.

Stable boundary traversal uses dense edge, vertex, and triangle mark arrays with
epoch counters. Repeating a directed edge side, vertex state, or triangle in one
trace returns a typed traversal error. Hash iteration order is not involved.

## Raw graph construction

For each cached stable polygon, an edge whose midpoint has two or more tied
stable phases becomes one local fragment for each applicable canonical phase
pair. Shared sampling-edge intersections are registered once per pair and edge;
adjacent triangles must reproduce the same canonical edge parameter.

Sampled triple ties remain useful deterministic seeds, but they are not the
only source of an interior invariant. Every stable local phase-pair fragment
also searches its triangle and edge-adjacent triangle patch for each possible
third phase. A safeguarded continuous solve evaluates `T_P-T_Q = 0` and
`T_P-T_R = 0` through the same prepared source evaluators used by projection.
It accepts a root only when all three values are finite, the root is inside its
permitted patch, pair residuals are within the configured tolerance, and no
participating competitor is higher than the common temperature. This preserves
short node-to-node branches even when no affine fragment endpoint was initially
classified as a sampled triple tie.

An accepted continuous node is attached to all compatible pair branches. When
it occurs in a local fragment interior, that fragment is split into two
nonzero child fragments. When it refines a sampled triple-tie endpoint, the
matching local endpoint is relocated to the continuous coordinate instead of
retaining the affine sampled coordinate as a second node. The sampled endpoint
is only a bounded topology seed; continuous residuals and the full-precision
node coordinate remain authoritative. Endpoint relocation is deterministic and
requires the same complete phase set in the local patch. Endpoint identity
remains the full canonical node ID and full-precision composition. A coarse
composition signature, when reported by diagnostics, is never used to merge
nodes.

Before adjacency is built, every accepted interior node is reevaluated through
the prepared continuous source evaluators. All participating values must be
finite, the maximum equality residual must satisfy
`StableBoundaryOptions::temperature_tolerance`, and no nonparticipating phase
may exceed the common temperature beyond `stability_tolerance`. The resulting
`StableInvariantVerification` records phase values, equality residual, and
stability margin. A sampled triple tie that cannot pass this check is rejected
rather than silently becoming graph topology.

Every fragment incident to a node creates a deterministic pending half-edge.
Traversal consumes the lowest pending key first, orients each fragment away
from the current endpoint, and selects the best forward continuation at shared
grid features without immediate retracing. A path is committed only after it
reaches another canonical node.

Construction order is therefore:

```text
binary discovery
    -> cached local stable fragments
    -> canonical outer and interior nodes
    -> raw node-to-node traversal
    -> incidence and topology validation
```

No cleanup or projection participates in phase-pair choice, triangle traversal,
third-phase interruption, invariant discovery, or graph connectivity.

## Optional shared path regularization

`StableBoundaryOptions::regularization` accepts the same neutral
`PathRegularizationOptions` used through the compatibility name
`ContourRegularization` by ordinary regular and irregular contours. The shared
layer owns:

- finite-coordinate normalization and boundary snapping;
- consecutive duplicate and zero-length removal;
- open/closed path conventions;
- arclength in the canonical equilateral plane;
- deterministic chord redistribution;
- damped implicit projection and configurable backtracking;
- fixed open-path endpoint restoration.

The equation callback remains quantity-specific. Isotherms use `T_p-L`;
univariants use `T_p-T_q`. The univariant callback evaluates the original
prepared sources for pair residual and stable-envelope ownership, while the
common sampling-grid affine model supplies the local difference gradient when a
source evaluator has no analytic gradient API. Every correction explicitly
relocates in the regular grid, so crossing several sampling triangles is allowed.

A candidate is rejected when either pair phase is undefined, a competitor is
higher, the simplex is left, or the correction leaves the raw segment
neighbourhood. Step damping retries a smaller correction. Exhaustion returns a
typed undefined, unstable, branch-switch, zero-gradient, or non-convergence
error.

The start and end invariant nodes are immutable. After projection, the
implementation restores their exact graph-owned coordinates and revalidates:

- phase pair and terminal node IDs;
- node incidence;
- finite normalized compositions;
- pair residual and stable ownership;
- absence of duplicate or reversed segments;
- absence of a newly introduced self-intersection.

Regularization is strictly post-topology. If a recoverable regularization
attempt fails, the complete raw branch remains in the network and the failure
is recorded as `StableUnivariantRegularizationFailure`. Each path reports an
effective `StablePathGeometryState`: `Raw`, `Regularized`, or `RawFallback`.
Consequently a projection can be partially regularized without losing its
invariants, univariants, or graph incidence.

## Complexity

Let `P` be phase count, `V`, `E`, and `T` the regular sampling-grid topology
sizes, `B` the configured samples over all binary boundaries, and `F` the number
of local stable-boundary fragments.

- dense topology construction is `O(V + E + T)` memory and time;
- cached binary full sweeps are `O(P*B)` plus pair root refinements;
- fragment collection is linear in cached stable-polygon edges;
- seeded traversal is `O(F)` for emitted boundary-connected components;
- raw path storage is linear in emitted points;
- regularization cost is proportional to redistributed points times projection
  iterations and phase evaluations.

No public wall-clock performance claim is made.

## Public workflow

```rust
use ternary_contours::{
    PathRegularizationOptions, StableBoundaryOptions,
};

# fn run(prepared: &ternary_contours::PreparedStablePhaseEnsemble<'_>)
#     -> Result<(), Box<dyn std::error::Error>> {
let raw = prepared.stable_boundaries(StableBoundaryOptions::default())?;
let regularized = prepared.stable_boundaries(StableBoundaryOptions {
    regularization: Some(PathRegularizationOptions {
        spacing: 0.02,
        ..PathRegularizationOptions::default()
    }),
    ..StableBoundaryOptions::default()
})?;

assert_eq!(raw.nodes, regularized.nodes);
for path in &regularized.univariants {
    assert_eq!(path.points[0], regularized.nodes[path.start.0].point());
    assert_eq!(path.points.last(), Some(&regularized.nodes[path.end.0].point()));
}
# Ok(()) }
```

The complete executable uses three partially defined affine phases and prints
binary traces, graph nodes, raw paths, and regularization diagnostics:

```text
cargo run --release --example stable_boundary_network
```

## Deferred work

- discovery and tracing of isolated closed univariant loops;
- exact clipping of arbitrary phase-domain boundaries inside a sampling cell;
- local hierarchical sampling-grid refinement;
- direct nonlinear/cubic stable-envelope topology;
- stable filled regions and cubic-alpha filled bands;
- rendering adapters and renderer coordinates;
- a neutral C ABI and language bindings.

## Ternary invariant multiplicity

The liquid phase is implicit. An interior stable invariant therefore represents
Liquid plus exactly three condensed liquidus phases. The network never creates a
four-solid interior node. Nearby continuous solves are merged only when their
three sorted phase IDs, full-precision root, temperature, and residual evidence
are compatible. Distinct three-phase roots may occupy one sampling triangle and
may share a phase pair; any number of such nodes is supported. If authoritative
continuous evaluations prove four or more phases tied and stable at one point,
construction returns `StableBoundaryError::OverdeterminedTernaryInvariant` with
all phase values and equality residuals instead of accepting invalid topology.

## Topology reuse and convergence diagnostics

Stable topology is keyed by source data, interpolation configuration, sampling,
and topology-affecting regularization settings. Changing only isotherm range or
levels reuses the accepted boundary graph and rebuilds contour paths only. The
projection diagnostics expose topology-build, topology-reuse, and isotherm
rebuild counters. A retained graph is not regularized again for a level-only
update.

`StableTopologySignature` compares graph topology without transient IDs.
`TopologyOnly` compares phase sets and incidence, `ToleranceAwareGeometry` adds
documented comparison quantization, and `ExactDiagnostic` retains exact stored
floating-point geometry. Raw and regularized networks must agree under the
topology-only comparison; per-path geometry remains `Raw`, `Regularized`, or
`RawFallback`.
