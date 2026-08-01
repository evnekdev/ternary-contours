# Ternary mesh and scalar-field metrics

`ternary-contours::metrics` provides deterministic numerical measurements in the
canonical logical equilateral plane:

```text
A = (0, 0)
B = (1, 0)
C = (1/2, sqrt(3)/2)
```

Public positions remain semantic `[a, b, c]`; the logical conversion is used
only to make distances, directions, gradients, Hessians, and shape measures
invariant under display projection.

## Capability boundary

Triangulation-quality metrics describe irregular Delaunay meshes only.
Gradient, Hessian, curvature, derived-field, interpolation-response, and
contour-response metrics apply to both regular and irregular fields.

| Metric family | Regular grid | Irregular mesh |
| --- | --- | --- |
| Triangle-quality distribution | Not needed; analytically uniform | Yes |
| Delaunay topology and valence | No | Yes |
| Gradient and gradient norm | Yes | Yes |
| Interior-edge gradient jumps | Yes | Yes |
| Local quadratic Hessian estimate | Yes | Yes |
| Curvature anisotropy | Yes | Yes |
| Derived-field evaluation | Yes | Yes |
| Mesh–field alignment | Controlled lattice form | Full Delaunay form |
| Alpha response | Regular cubic continuity | Irregular alpha and continuity |
| Contour response | Yes | Yes |

## Shared field quantities

`TernaryGradient` preserves the established reduced semantic gradient
`[g_a, g_b]`, with `c = 1-a-b`. Its canonical logical representation is:

```text
g_x = g_b - g_a
g_y = -(g_a + g_b) / sqrt(3)
||g||^2 = (4/3) (g_a^2 - g_a g_b + g_b^2)
```

Use `FieldSample::gradient()` or `IrregularFieldSample::gradient()` to obtain
this shared type. `direction()` is `[0, 0]` for a zero gradient and
`direction_if_nonzero()` makes that case explicit. `directional_derivative`
uses a caller-supplied logical direction; pass a unit direction for units of
scalar value per unit logical distance.

`DerivedRegularTernaryField` and `DerivedIrregularTernaryField` wrap prepared
evaluators. They expose `Value`, reduced and logical gradient components, and
`GradientNorm`, with the same `value`, `evaluate`, `value_at_location`,
`values`, and `values_into` shape as their source evaluator. They never rebuild
prepared cubic intervals or repeat location for a supplied location.

`GradientJump` is the common interior-edge record. It reports explicit left and
right one-sided gradients, the total logical jump magnitude, and signed
normal/tangential components. The tangent is the canonical low-ID to high-ID
edge direction; the left side is the triangle whose third point lies to its
left. An affine field therefore has zero jumps. A cubic-alpha field is checked
for shared-edge value and tangential-gradient agreement; its normal derivative
may differ, because it is C0 rather than generally C1.

## Local quadratic sampled-field estimates

`LocalQuadraticEstimate` is interpolation-independent. It fits vertex samples
in the logical plane to a centred quadratic using scaled modified
Gram–Schmidt QR; it does **not** form normal equations. It reports fitted
first derivatives, logical Hessian, Frobenius norm, Laplacian, determinant,
eigenvalues, deterministically signed principal direction, anisotropy,
residual RMS, QR diagonal condition estimate, sample count, and ring used.

`RegularTernaryScalarField::local_quadratic_estimate` expands deterministic
integer-lattice rings. `IrregularTernaryScalarField::local_quadratic_estimate`
expands deterministic stable-ID graph rings. Both return the same result and
typed `LocalQuadraticError` for insufficient data, rank deficiency,
ill-conditioning, invalid options, or non-finite input. An affine sampled
field has an approximately zero Hessian; exact quadratics are recovered to the
conditioning permitted by their local sample geometry.

These estimates are intentionally distinct from analytic prepared-interpolant
gradients. Analytic Hessians are not exposed in this milestone, so no API
implies that a local sampled-field Hessian is an interpolant-specific second
derivative.

## Irregular Delaunay geometry

With `irregular-delaunay`, `IrregularTernaryMesh::metrics()` returns stable
triangle-, edge-, vertex-, and whole-mesh records. They include triangle area,
edge lengths, angle bounds, radius and mean ratios, altitude aspect ratio,
shape-tensor anisotropy and direction; edge area/size gradation; vertex
valence, barycentric dual area, nearest-neighbour length, incident-area spread,
and graph distance to the convex hull; and hull/topology distributions.

`IrregularTernaryMesh::incident_edges` is a stable adjacency view retained for
these metrics and for the future irregular edge-alpha implementation. It does
not expose a `delaunay` handle.

`IrregularTernaryScalarField::triangle_field_alignment` combines triangle
shape axes with averaged successful local Hessian directions. The regular
adapter deliberately does not manufacture equivalent Delaunay-quality
variation; `RegularFieldMetrics` instead records the known equal-area triangle
family and prepared gradients as a controlled reference.

## Response and distributions

`DistributionSummary` gives explicit empty-distribution semantics (`count=0`
and absent scalar values), deterministic inclusive quantiles, and clear
constant/zero-mean coefficient-of-variation behaviour. `Histogram` supports
equal-width, logarithmic, quantile, or explicit finite edges and explicitly
counts underflow and overflow. `MetricWeighting` records the intended basis;
the initial field metric summaries are explicitly unweighted.

`ContourSet::response_metrics` and `IrregularContourSet::response_metrics`
measure final path count, point count, and canonical logical arc length without
modifying extraction. The irregular variant additionally associates the final
per-level adaptive and spacing diagnostics already produced by contouring.

With `irregular-cubic-alpha`,
`InterpolatedIrregularTernaryField::irregular_alpha_response_metrics()` reports
one final record per canonical edge: logical length, endpoint alpha
coefficients, compact complete-stencil versus linear-fallback flag, and hull
classification. It does not retain virtual locations or stencil geometry.

## Limitations

The irregular domain remains the supplied samples' convex hull. There are no
holes, constrained edges, mesh optimization, refinement, irregular filled
bands, analytic Hessians, C ABI, or rendering additions here. The later
irregular cubic-alpha milestone can use the retained canonical edges, incident
triangles, dense IDs, point locations, and compact stencil flags without
changing the public mesh model.