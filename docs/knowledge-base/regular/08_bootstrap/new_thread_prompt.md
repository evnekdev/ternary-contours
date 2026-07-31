# Bootstrap Prompt for a New Chat or Coding Agent

Use the following prompt together with this knowledge-base directory.

---

You are working on a numerical interpolation and contouring method for scalar values sampled on a regular ternary composition grid.

Read every Markdown file in this knowledge base before proposing code or formulas.

Core requirements:

1. The immediate implementation uses the natural regular triangular lattice in 2-D. Do not use Delaunay triangulation, irregular meshes, or Kuhn simplices.
2. Conventional linear contouring remains a baseline and contains no alpha terms.
3. Cubic-alpha intervals use
   `y(t)=y0*(1-t)+y1*t+(1-t)*t*(alpha0+alpha1*t)`.
4. `alpha1` multiplies the directed parameter `t`.
5. Reversing an interval transforms
   `(alpha0,alpha1)` to `(alpha0+alpha1,-alpha1)`.
6. One unique regular-grid edge has one alpha interval, shared by adjacent triangles.
7. The local triangle field is
   `f = sum(fi*xi) + sum(Eij)`.
8. Every pair contribution is
   `Eij = xi*xj*(alpha0+alpha1*tij)`.
9. The prefactor is always raw `xi*xj`, never normalized `Xi*Xj`.
10. Muggianu uses
    `tij = 1/2 + (xj-xi)/2 = xj+xk/2`.
11. Kohler uses
    `tij = xj/(xi+xj)` and the contribution is exactly zero at `xi=xj=0`.
12. Both policies reproduce the same one-dimensional spline on the binary edge.
13. If `alpha1=0`, Muggianu and Kohler coincide.
14. The nonlinear local field may have complex contour topology. Use robust adaptive barycentric subdivision rather than assuming one segment per triangle.
15. Regularize final paths by approximate equal-arclength redistribution followed by damped normal/Newton projection back to `f=level`.
16. Projection may cross elementary-triangle boundaries and must then use the new containing triangle's local field.
17. Keep numerical interpolation, contour extraction, and projection autonomous from Plotters.
18. Design the interpolation module so it can later be extracted and generalized to an N-component/Kuhn-simplex crate, but do not add Kuhn-simplex code now.
19. Verify analytic gradients against finite differences.
20. Never silently hide unresolved topology, projection failure, boundary stencil fallback, or NaN conditions.

Before implementation, summarize the mathematical conventions and list any ambiguity you find. Do not reinterpret Muggianu as raw `xj`; it is the centered/perpendicular construction described above.

---
