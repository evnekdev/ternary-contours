# Changelog

All notable changes to this project are documented here.

## 0.1.0 - 2026-07-31

First release of `ternary-contours`.

- Canonical regular A/B/C ternary-grid generation and validated scalar fields.
- Backend-independent piecewise-linear isolines with deterministic path assembly.
- Optional cubic-alpha line contours using Akima, MAKIMA, PCHIP, and Steffen edge intervals.
- Muggianu and Kohler binary extrapolation policies; RawBarycentric is experimental only.
- Arc-length regularization, implicit level projection, and bounded topology diagnostics.
- Piecewise-linear filled contour bands with deterministic region and hole geometry.

Known limitations: the crate supports regular two-dimensional ternary grids only. It does not provide irregular triangulation, N-component grids, filled cubic-alpha bands, rendering, viewport clipping, colour maps, or labels. Cubic-alpha fields are C0 rather than C1 across grid edges.
