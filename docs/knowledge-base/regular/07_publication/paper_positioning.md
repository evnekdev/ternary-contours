# Publication and Engineering Positioning

## Strongest contribution claim

Avoid claiming that cubic interpolation, Muggianu/Kohler extrapolation, adaptive subdivision, or predictor-corrector projection are individually new.

A defensible engineering contribution is:

> A modular, edge-derived cubic contour-reconstruction workflow for regular ternary grids that exactly reuses shape-preserving one-dimensional edge splines, supports thermodynamically meaningful Muggianu/Kohler continuation, resolves nonlinear contour topology, and produces approximately equal-arclength level-preserving paths in reusable open-source software.

## Likely publication routes

### Engineering/scientific-computing paper

Emphasize:

- missing end-to-end capability in mainstream packages;
- reproducible algorithm;
- thermodynamic use cases;
- comparison against realistic MATLAB/Python/R/Octave workflows;
- accuracy, robustness, runtime, and usability.

### Software paper

Emphasize:

- statement of need;
- open-source implementation;
- tests and CI;
- documentation;
- examples;
- reusable numerical API;
- cross-language access.

A detailed engineering methods paper and a shorter software paper can be separate outputs.

## Mainstream comparison targets

Potential baselines:

- MATLAB matrix contour workflows and triangular-contour add-ons;
- Matplotlib `tricontour`;
- SciPy Clough-Tocher or other smooth interpolation followed by contouring;
- R ternary packages;
- Octave triangular contours.

Compare capabilities and workflow, not merely whether the underlying mathematics could be manually reproduced.

## Essential experimental evidence

- analytic benchmark fields;
- thermodynamic case studies;
- grid convergence;
- level residual;
- geometric contour error;
- spacing uniformity;
- overshoot behavior;
- topology failures;
- runtime;
- ablation study;
- Muggianu versus Kohler comparison;
- Python/MATLAB wrapper usability if available.

## Novelty risk

A reviewer may describe the method as a combination of known components. Counter this by demonstrating identifiable new engineering properties:

- exact edge-spline reproduction with compact storage;
- shared-edge consistency;
- clean separation of one-dimensional interpolation method from interior extrapolation geometry;
- field-constrained rather than free geometric smoothing;
- reusable implementation across languages;
- measurable gains on coarse engineering data.
