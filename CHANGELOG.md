# Changelog

All notable changes to this project are documented here.

## Unreleased

- Added backend-independent irregular Delaunay isolines for prepared linear
  fields and converged irregular cubic-alpha fields.
- Added deterministic mesh-edge ownership, canonical shared cubic-edge roots,
  scale-aware adaptive barycentric extraction, and field-aware regularization
  with global convex-hull-aware projection.
- Centralized canonical equilateral logical geometry for Delaunay construction
  and contour arclength measurements.
## 0.1.0 - 2026-07-31

First release of `ternary-contours`.

- Canonical regular A/B/C ternary-grid generation and validated scalar fields.
- Backend-independent piecewise-linear isolines with deterministic path assembly.
- Optional cubic-alpha line contours using Akima, MAKIMA, PCHIP, and Steffen edge intervals.
- Muggianu and Kohler binary extrapolation policies; RawBarycentric is experimental only.
- Arc-length regularization, implicit level projection, and bounded topology diagnostics.
- Piecewise-linear filled contour bands with deterministic region and hole geometry.

Known limitations: the crate supports regular two-dimensional ternary grids only. It does not provide irregular triangulation, N-component grids, filled cubic-alpha bands, rendering, viewport clipping, colour maps, or labels. Cubic-alpha fields are C0 rather than C1 across grid edges.
