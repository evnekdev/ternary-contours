# Project Context and Design Intent

## Immediate project

The current host project is `plotters-ternary`, a Rust extension around Plotters for publication-quality ternary charts. Earlier milestones provide:

- ternary composition and projection geometry;
- rectangular viewport clipping;
- full and cropped charts;
- lines, points, legends, scientific markers;
- polygons, phase regions, and text annotations;
- publication-quality ternary axes;
- PNG/SVG rendering infrastructure.

Milestone 7 adds line contours over regular ternary scalar grids.

## Primary engineering use cases

The method is intended for scalar properties over ternary composition space, including:

- excess Gibbs energy;
- activity or chemical-potential-derived fields;
- liquidus temperature;
- phase fraction;
- enthalpy, density, viscosity, or other engineering properties;
- fields produced by thermodynamic simulation at regular composition-grid points.

## Core motivation

Popular engineering packages generally provide either:

- piecewise-linear triangular contours; or
- generic smooth surface interpolation; or
- ternary plotting facilities.

They do not normally expose one integrated workflow with:

- shape-preserving one-dimensional cubic intervals along all three ternary lattice directions;
- exact shared-edge reuse;
- selectable Muggianu/Kohler interior continuation;
- topology-aware nonlinear contour extraction;
- approximately uniform contour-point spacing;
- projection back to the exact interpolated isolevel;
- publication-ready ternary rendering.

## Architectural intent

The numerical method must not be inseparable from Plotters. The implementation should begin as an autonomous submodule and later be extractable into a numerical core crate.

Longer-term targets may include:

- a stable Rust numerical core;
- C ABI;
- Python bindings;
- MATLAB MEX wrapper;
- Fortran `ISO_C_BINDING` wrapper;
- a separate N-component implementation on simplex/Kuhn grids.

## Important boundary

For the current ternary implementation, use the natural regular triangular lattice. Do not use Kuhn simplices in 2-D merely for future generality. In 2-D, regular elementary triangles are more symmetric and less elongated. Kuhn simplices become useful when a uniform construction across dimensions is required.
