# Contour Extraction Pipeline

## Linear baseline

For every requested level and every elementary triangle:

1. classify corner values relative to the level with tolerance;
2. identify crossed edges;
3. locate crossings by linear interpolation;
4. create zero or one local segment for ordinary nondegenerate cases;
5. use deterministic ownership for vertices or complete edges on the level;
6. join segments into open paths and closed loops.

For an edge:

\[
t=\frac{L-z_0}{z_1-z_0}.
\]

Use exact endpoints when the level matches a vertex within tolerance.

## Cubic-alpha topology

A nonlinear local field may contain:

- multiple roots on one edge;
- tangencies;
- more than two boundary crossings;
- multiple branches;
- an interior closed loop with no boundary crossing.

Do not force ordinary marching-triangle topology onto the whole elementary triangle.

## Preferred robust method: adaptive microtriangulation

For each elementary triangle and level:

1. evaluate the exact local cubic-alpha field at the triangle vertices;
2. subdivide the barycentric triangle deterministically;
3. evaluate generated midpoint or child vertices with the exact field;
4. use error/sign/flatness criteria to decide whether to refine further;
5. apply linear marching triangles to accepted microtriangles;
6. assemble local segments with stable subdivision endpoint keys;
7. report unresolved topology at maximum depth.

Potential refinement indicators:

- mismatch between edge cubic roots and linear child-edge crossings;
- sign patterns suggesting unresolved extrema;
- geometric deviation between parent and child contour approximations;
- large field curvature or variation;
- tangency candidates;
- user-specified maximum segment deviation.

## Optional exact edge roots

One-dimensional cubic edge roots can improve boundary accuracy:

1. locate roots of the derivative;
2. split `[0,1]` into monotone intervals;
3. bracket every level crossing;
4. solve with a robust bracketed method;
5. retain all roots in `[0,1]`;
6. detect near-tangent roots.

Do not assume one root per edge and do not rely solely on Cardano formulas.

## Path assembly

Represent local endpoints with stable topological keys where possible. Build endpoint-to-segment adjacency.

Expected degrees:

- 1 at an open domain boundary;
- 2 on a regular interior contour.

Degree greater than two must be diagnosed as unresolved branching or invalid topology.

Extract open paths first from degree-1 endpoints, then closed loops from remaining unused segments.

Canonicalize path order for deterministic tests.

## Clipping

Compute complete logical paths before rectangular viewport clipping.

Then reuse the chart's existing line clipping to split paths into visible subpaths.

Never contour in screen pixels and never treat the viewport rectangle as a Cartesian axis frame.
