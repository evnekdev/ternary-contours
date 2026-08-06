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

The binary root is authoritative. A sampling-grid trace reaching the same outer
boundary is attached to the nearest compatible phase-pair node within one final
sampling interval and its endpoint is replaced by the canonical binary
coordinate.

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

Interior endpoints with three or more stable phases are merged by composition
and temperature tolerances into one graph node. Every fragment incident to a
node creates a deterministic pending half-edge. Traversal consumes the lowest
pending key first, orients each fragment away from the current endpoint, and
selects the best forward continuation at shared grid features without immediate
retracing. A path is committed only after it reaches another canonical node.

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

Regularization retains only the requested final coordinates. Its diagnostics
record summary counts and lengths rather than a second complete copy of the raw
path.

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