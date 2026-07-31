# Decision Log

## Accepted

- Current contour input is a regular ternary scalar grid.
- No external triangulation dependency.
- Linear interpolation remains available.
- Cubic edge coefficients come from `spline1d` alpha APIs.
- Initial cubic methods: Akima, Makima, PCHIP, Steffen.
- The alpha convention is `alpha0 + alpha1*t`.
- Canonical directed edges are required.
- Muggianu and Kohler are explicit user-selectable policies.
- The pair prefactor remains `x_i*x_j` for both policies.
- Nonlinear topology uses adaptive microtriangulation.
- Contour points may be redistributed approximately uniformly and projected back onto the level set.
- Numerical code must be modular and Plotters-independent.
- Future ND work belongs in a separate Kuhn/simplex crate.

## Rejected or deferred

- Delaunay triangulation.
- Irregular scattered-point contouring.
- User-supplied arbitrary triangle meshes in the first version.
- Kuhn simplices in the ternary 2-D implementation.
- Filled contours in the line-contour milestone.
- Free geometric smoothing that does not enforce `f=level`.
- Assuming one cubic root per edge.
- Assuming one contour segment per nonlinear triangle.
- Claiming full `C1` continuity without proof.
