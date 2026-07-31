# Linear filled-contour kernel

ContourBandSet is the backend-independent isoband API for
RegularTernaryScalarField. Callers supply finite, strictly increasing scalar
breaks. For l0 < l1 < ... < lm, scalar ownership is exact and half-open:

- lower extreme: f < l0;
- intermediate band i: li <= f < li+1;
- upper extreme: f >= lm.

The closed polygons used to represent adjacent bands may share a threshold
curve. That is a zero-area geometric boundary, not a positive-area overlap.

Each elementary composition triangle is clipped against its two scalar bounds
with exact affine edge interpolation. The core retains deterministic,
non-overlapping simple fragments as well as assembled ContourRegion values.
Fragments are the portable fill representation: drawing only them leaves
region holes transparent without a background-colour paint-over.

Shared fragment edges cancel in a deterministic, neighbouring-cell-aware
composition-space canonicalisation pass. Ring containment, not input
orientation alone, assigns holes. Exterior rings are normalised CCW and holes
CW in semantic (a,b) coordinates. Degenerate and zero-area fragments are
removed using composition-space tolerances, never rendering tolerances.

Only piecewise-linear bands are implemented. A cubic-alpha request returns an
unsupported interpolation error; no silent fallback occurs. The core contains
no colour, opacity, viewport, screen-space, or Plotters code.
