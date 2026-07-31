# Irregular triangulations: self-consistent edge-alpha estimation

## Scope

This note extends the ternary cubic contour knowledge base from regular simplex grids to irregular triangular meshes, including Delaunay triangulations. It focuses on constructing and refining one-dimensional edge interpolation data from an irregular two-dimensional scalar field.

The key difficulty is that, for an irregular mesh, the two vertices opposite a shared edge are generally not collinear with that edge. Using those vertices directly in a one-dimensional edge stencil mixes tangential variation along the edge with normal variation caused by the off-line displacement.

The proposed method removes this contamination by defining canonical collinear virtual stencil points and evaluating their scalar values through the triangular field.

## 1. Canonical virtual stencil for an edge

Let an oriented mesh edge have endpoints

\[
\mathbf{x}_0,\qquad \mathbf{x}_1,
\]

with edge vector and length

\[
\mathbf{d}=\mathbf{x}_1-\mathbf{x}_0,
\qquad
L=\|\mathbf{d}\|.
\]

Define two virtual neighbours by extending the edge line by one edge length beyond each endpoint:

\[
\mathbf{x}_{-}=\mathbf{x}_0-\mathbf{d}=2\mathbf{x}_0-\mathbf{x}_1,
\]

\[
\mathbf{x}_{+}=\mathbf{x}_1+\mathbf{d}=2\mathbf{x}_1-\mathbf{x}_0.
\]

The resulting stencil is uniformly spaced and exactly collinear:

\[
\mathbf{x}_{-},\quad \mathbf{x}_0,\quad \mathbf{x}_1,\quad \mathbf{x}_{+}.
\]

This construction is independent of the locations of the two vertices opposite the edge and therefore avoids direct off-axis contamination from mesh irregularity.

## 2. Containing-triangle evaluation

The virtual points should not be evaluated by extrapolating from the triangles incident to the central edge unless those triangles actually contain the points.

Instead, for each virtual point:

1. locate the triangle that geometrically contains it;
2. evaluate the current triangular scalar-field interpolant in that triangle;
3. use the resulting value as the virtual stencil value.

Let

\[
T_- \ni \mathbf{x}_-,
\qquad
T_+ \ni \mathbf{x}_+.
\]

At iteration \(n\), define

\[
f_-^{(n)}=I_{T_-}^{(n)}(\mathbf{x}_-),
\qquad
f_+^{(n)}=I_{T_+}^{(n)}(\mathbf{x}_+).
\]

The containing triangles may be adjacent to the central edge, several triangles away, or geometrically remote in terms of adjacency. The dependency graph is therefore not the same as the ordinary triangle-neighbour graph.

## 3. Linear bootstrap

The first estimate should use the piecewise-linear field over the triangulation:

\[
f_-^{(0)}=I_{T_-}^{\mathrm{lin}}(\mathbf{x}_-),
\qquad
f_+^{(0)}=I_{T_+}^{\mathrm{lin}}(\mathbf{x}_+).
\]

Together with the actual edge endpoint values,

\[
f_0=f(\mathbf{x}_0),
\qquad
f_1=f(\mathbf{x}_1),
\]

this gives the uniform four-point stencil

\[
f_-^{(0)},\quad f_0,\quad f_1,\quad f_+^{(0)}.
\]

The selected one-dimensional interpolation method can then construct the initial edge-alpha data.

Using the actual containing triangle is particularly useful when the central edge belongs to a large triangle but a virtual point falls inside a smaller, well-shaped triangle. For a smooth field, the local piecewise-linear estimate in the smaller triangle is generally more accurate than extrapolation from the larger incident triangle. Triangle shape quality remains important: a small but highly degenerate triangle may still give a poor estimate.

## 4. Self-consistent alpha field

The alpha coefficients determine the curved triangular field, while that triangular field is used to evaluate the virtual stencil values from which the alpha coefficients are reconstructed. This naturally defines a fixed-point problem.

Let \(\boldsymbol{\alpha}\) denote the complete edge-alpha field. One global update is an operator

\[
\widehat{\boldsymbol{\alpha}}^{(n+1)}
=
\mathcal{F}\!\left(\boldsymbol{\alpha}^{(n)}\right),
\]

where \(\mathcal{F}\) performs the following operations:

1. construct the current cubic interpolant in every triangle from the current edge-alpha values;
2. evaluate every precomputed virtual point through the interpolant of its containing triangle;
3. reconstruct a candidate alpha value for every edge from its updated four-point collinear stencil.

A converged field satisfies

\[
\boldsymbol{\alpha}^{*}
=
\mathcal{F}\!\left(\boldsymbol{\alpha}^{*}\right).
\]

This is best interpreted as a self-consistency condition between the edge splines and the triangular field they induce.

## 5. Synchronous global sweeps

The reference implementation should begin with complete synchronous sweeps, analogous to Jacobi iteration.

For sweep \(n\):

1. freeze the complete alpha field \(\boldsymbol{\alpha}^{(n)}\);
2. evaluate all virtual points using only that frozen field;
3. calculate all candidate values \(\widehat{\boldsymbol{\alpha}}^{(n+1)}\);
4. replace or relax the complete field simultaneously.

The undamped update is

\[
\boldsymbol{\alpha}^{(n+1)}
=
\widehat{\boldsymbol{\alpha}}^{(n+1)}.
\]

A relaxed update is

\[
\boldsymbol{\alpha}^{(n+1)}
=
(1-\omega)\boldsymbol{\alpha}^{(n)}
+
\omega\widehat{\boldsymbol{\alpha}}^{(n+1)},
\qquad 0<\omega\le 1.
\]

Synchronous sweeps have several advantages:

- deterministic results independent of edge traversal order;
- straightforward parallelization;
- clear residual measurement;
- a well-defined mathematical iteration;
- a reliable reference against which later local-update schemes can be tested.

Convergence is not automatically guaranteed for every triangulation and field. Damping, residual monitoring, iteration limits, and diagnostic reporting should therefore be part of the design.

## 6. Dependency graph

An edge does not depend only on its incident triangles.

For an edge \(e\), each virtual point lies in a containing triangle. Evaluating the cubic field in that triangle depends on the alpha data associated with that triangle's edges. Therefore one edge update depends on the edge-alpha values of the two containing triangles.

Conceptually:

```text
edge e
  |- virtual point x-
  |    `- containing triangle T-
  |         `- alpha values on the edges of T-
  `- virtual point x+
       `- containing triangle T+
            `- alpha values on the edges of T+
```

This induces a directed dependency graph:

- source nodes: triangle-edge alpha values used to evaluate a containing-triangle interpolant;
- dependent node: the central edge whose virtual value uses that interpolant.

The graph should be precomputed after locating all virtual points.

## 7. Precomputed geometric stencil data

All geometry required to locate and evaluate virtual points is independent of the alpha iteration and should be computed once.

A useful record is:

```rust
struct VirtualPointLocation {
    point: Point2,
    triangle: TriangleId,
    barycentric: [f64; 3],
}

struct EdgeVirtualStencil {
    edge: EdgeId,
    minus: VirtualPointLocation,
    plus: VirtualPointLocation,
}
```

Precomputation should include:

- the virtual point coordinates;
- the containing-triangle IDs;
- barycentric or equivalent local coordinates in those triangles;
- reverse dependency lists from each triangle edge to all central edges whose virtual values depend on it.

After this preprocessing, a global iteration requires no repeated spatial point-location search.

## 8. Residuals and stopping criteria

The iteration should report a residual over the full alpha field. A generic scaled form is

\[
r_n
=
\max_e
\frac{
\|\boldsymbol{\alpha}^{(n+1)}_e-
\boldsymbol{\alpha}^{(n)}_e\|
}{
\sigma_e+\|\boldsymbol{\alpha}^{(n)}_e\|
},
\]

where \(\sigma_e\) is an edge-dependent or global stabilizing scale.

Stopping conditions may include:

- residual below tolerance;
- no meaningful changes in any edge;
- maximum iteration count;
- detected oscillation or divergence;
- stagnation over several sweeps.

Diagnostics should include at least:

- number of sweeps;
- final residual;
- number of unconverged edges;
- largest edge residual;
- damping factor used;
- whether the result is converged, stagnated, oscillatory, or iteration-limited.

## 9. Local dependency-driven updates

Once the synchronous implementation is validated, it can be accelerated using a local active-set or work-queue method.

The key rule is that locality must follow the precomputed dependency graph, not only geometric adjacency.

A typical algorithm is:

1. initialize all edges as active;
2. pop an active edge and compute its candidate alpha value;
3. if the change exceeds tolerance, update it;
4. mark every dependent edge as dirty whose virtual point is evaluated in a triangle using the changed alpha;
5. continue until the queue is empty or another stopping condition is reached.

Conceptually:

```text
changed alpha on edge a
        |
        v
all triangles whose interpolants use a become dirty
        |
        v
all central edges with virtual points in those triangles become dirty
```

This is a graph-based Gauss-Seidel or asynchronous fixed-point solver.

Possible queue policies include:

- FIFO queue;
- largest-residual first;
- triangle batches;
- graph coloring for parallel conflict-free updates;
- generation counters to avoid duplicate queue entries.

## 10. Global sweeps versus local propagation

### Global synchronous sweeps

Use as the reference method because they are:

- deterministic;
- easy to parallelize;
- traversal-order independent;
- easy to validate mathematically and numerically.

### Local immediate updates

Potential benefits:

- fewer evaluations after most of the mesh has converged;
- faster propagation in strongly local dependency regions;
- natural residual-driven refinement.

Risks:

- traversal-order dependence;
- more difficult parallelization;
- possible oscillation in dependency cycles;
- harder reproducibility;
- more complicated diagnostics.

The recommended development order is:

1. implement and validate synchronous global sweeps;
2. build the explicit dependency graph;
3. implement a local work-queue solver;
4. compare its converged result against the synchronous reference;
5. add scheduling and parallelization only after numerical equivalence is established.

## 11. Hybrid strategy

A practical solver can combine both approaches:

1. perform several global sweeps from the linear bootstrap;
2. once the residual becomes localized, switch to a work queue;
3. periodically perform a full audit sweep to detect dependencies missed through tolerances or stale queue state;
4. stop only when both the queue is empty and the audit residual is below tolerance.

This preserves much of the determinism and reliability of global sweeps while reducing late-stage work.

## 12. Boundary and domain issues

A virtual point may lie outside the triangulated domain, especially near boundaries, holes, or non-convex regions. This requires an explicit boundary policy.

Possible policies include:

- mark the edge stencil incomplete and use a one-sided construction;
- shorten the virtual extension until a valid containing point is found;
- use a boundary extrapolation rule;
- use a reflected or constrained virtual value;
- exclude the edge from cubic refinement and retain a linear edge model.

The chosen policy must be part of the numerical model and included in diagnostics. It should not be silently inferred from triangle adjacency.

## 13. Triangle interpolant requirements

The iterative method assumes that a triangle interpolant can be evaluated at any point inside the triangle using the current edge-alpha field.

The triangle construction should satisfy:

1. exact interpolation of the three vertex values;
2. exact restriction to the designated one-dimensional spline on each edge;
3. identical shared-edge values from both incident triangles;
4. deterministic interior blending;
5. stable evaluation on shape-regular triangles;
6. explicit behavior for highly skewed or nearly degenerate triangles.

Global \(C^1\) continuity is not required for the first implementation. Exact shared-edge restrictions provide a globally \(C^0\) scalar field, which is sufficient for consistent iso-level construction.

## 14. Suggested data layout

A structure-of-arrays layout is suitable for large meshes:

```rust
struct IrregularAlphaField {
    edge_endpoints: Vec<[VertexId; 2]>,
    edge_alphas: Vec<EdgeAlpha>,
    stencils: Vec<EdgeVirtualStencil>,
    triangle_edges: Vec<[EdgeId; 3]>,
    dependents: Vec<Vec<EdgeId>>,
}
```

For parallel synchronous sweeps, keep old and candidate alpha fields separate:

```rust
struct AlphaSweepBuffers {
    current: Vec<EdgeAlpha>,
    candidate: Vec<EdgeAlpha>,
}
```

This prevents read/write races and preserves exact sweep semantics.

For local updates, add:

```rust
struct ActiveSet {
    queue: VecDeque<EdgeId>,
    queued: BitVec,
    residuals: Vec<f64>,
}
```

## 15. Validation plan

The method should be tested against analytic scalar fields sampled on both regular and irregular triangulations.

Recommended field families:

- affine fields, which should produce no artificial curvature;
- quadratic bowls and domes;
- anisotropic quadratic fields;
- saddle fields;
- smooth trigonometric fields;
- fields with known contour topology.

Recommended mesh families:

- regular triangular lattice;
- mildly perturbed regular lattice;
- strongly irregular Delaunay mesh;
- mixed large and small triangles;
- meshes containing skinny triangles;
- convex and non-convex domains.

Measure:

- virtual-value error;
- alpha error where an analytic reference exists;
- residual convergence history;
- agreement between synchronous and local solvers;
- contour displacement from analytic level sets;
- contour topology changes;
- sensitivity to edge orientation;
- sensitivity to damping;
- behavior near boundaries.

An affine field is an essential invariant: the iterative process should not introduce curvature when the underlying scalar field is exactly affine.

## 16. Recommended implementation sequence

### Phase A: geometry and bootstrap

- construct canonical virtual points;
- locate their containing triangles;
- precompute barycentric coordinates;
- evaluate the piecewise-linear field;
- construct initial edge-alpha values.

### Phase B: reference global solver

- construct cubic triangle interpolants from frozen alpha data;
- evaluate all virtual points;
- compute all candidate alpha values;
- apply optional damping;
- report residuals and convergence state.

### Phase C: dependency graph

- build reverse dependencies from triangle edges to central edges;
- verify dependency completeness;
- expose diagnostics for graph degree and cycles.

### Phase D: local solver

- implement a residual-driven work queue;
- compare against converged global sweeps;
- add deterministic scheduling mode;
- add periodic audit sweeps.

### Phase E: iso-level integration

- use the converged irregular triangular field as the numerical source for iso-level extraction;
- keep path construction, regularization, and level projection in the numerical contour crate;
- pass only final contour coordinates to rendering adapters.

## 17. Architectural boundary

The irregular alpha-estimation machinery belongs in the numerical contour core, not in the plotting adapter.

```text
contour core
    irregular triangulation
    -> virtual stencil construction
    -> containing-triangle lookup
    -> linear bootstrap
    -> global or local alpha refinement
    -> triangular field evaluation
    -> iso-level extraction
    -> path construction
    -> regularization and level projection
    -> final contour coordinates

plotting adapter
    final contour coordinates
    -> chart-coordinate projection
    -> visual clipping
    -> styling and rendering
```

The plotting layer must not recalculate alpha values, alter numerical contour topology, smooth the field, or project contour points back onto an iso-level.

## 18. Summary

The proposed irregular-mesh method is a self-consistent edge-spline reconstruction:

- each edge defines a canonical, uniformly spaced, collinear virtual stencil;
- virtual values are evaluated in the triangles that actually contain the virtual points;
- the first estimate uses the piecewise-linear triangulated field;
- subsequent estimates use the curved field induced by the previous alpha pass;
- synchronous global sweeps provide the deterministic reference algorithm;
- local graph-driven updates provide a later acceleration;
- the true dependency graph is determined by containing triangles, not merely mesh adjacency;
- geometric lookup and dependency information should be precomputed;
- the converged field feeds the numerical iso-level extraction pipeline.
