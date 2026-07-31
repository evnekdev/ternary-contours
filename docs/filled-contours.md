# Linear filled-contour kernel

ContourBandSet is the backend-independent isoband API for
RegularTernaryScalarField. It accepts finite ordered scalar breaks and clips
each elementary composition triangle twice using exact affine edge
interpolation. The resulting fragments are joined by tolerance-keyed directed
boundary edges. Opposite shared fragment edges cancel, leaving deterministic
open rings.

The classification convention is lower-inclusive and upper-exclusive.
Adjacent polygons can share their closed geometric boundary but never overlap
in positive area. Exterior rings are counter-clockwise in (a,b) coordinates;
holes are clockwise. Degenerate and zero-area fragments are removed using
composition-space tolerances, never rendering tolerances.

Only piecewise-linear bands are implemented. A cubic-alpha request returns an
unsupported interpolation error; no silent fallback occurs. The core contains
no colour, opacity, viewport, screen-space or Plotters code.