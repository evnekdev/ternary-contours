# References and Source Pointers

This list is a starting point, not an exhaustive literature review.

## Thermodynamic geometric extrapolation

- F. Kohler, “Zur Berechnung der thermodynamischen Daten eines ternären Systems aus den zugehörigen binären Systemen,” *Monatshefte für Chemie*, 91, 738–740 (1960). DOI: 10.1007/BF00899814.
- “Muggianu and Toop-Muggianu interpolations...” *CALPHAD*, 6(1), 57–63 (1982). DOI: 10.1016/0364-5916(82)90016-5.
- P. Chartrand and A. D. Pelton, “A general ‘geometric’ thermodynamic model for multicomponent solutions,” *CALPHAD*, 25(2), 319–328 (2001). DOI: 10.1016/S0364-5916(01)00052-9.
- A. D. Pelton and P. Chartrand, “The Modified Quasi-Chemical Model: Part II. Multicomponent Solutions,” *Metallurgical and Materials Transactions A*, 32, 1355–1360 (2001). DOI: 10.1007/S11661-001-0226-3.

## Practical interpretation

Standard thermodynamic descriptions emphasize:

- Muggianu preserves a centered composition difference and is associated with symmetric/perpendicular projection;
- Kohler preserves the binary component ratio;
- interpolation polynomial choice and geometric extrapolation choice are separate decisions;
- for pair contributions, normalized binary evaluation is combined with a multicomponent weighting, which in the present alpha formulation yields the required raw `x_i*x_j` prefactor.

## One-dimensional implementation

- `evnekdev/spline1d`, especially `src/alpha.rs` and single-interval alpha APIs for Akima, Makima, PCHIP, and Steffen.
- Canonical formula:
  `y(t)=y0*(1-t)+y1*t+(1-t)*t*(alpha0+alpha1*t)`.

## Literature-review search terms

- triangular cubic contour interpolation
- contouring piecewise polynomial triangular fields
- implicit curve continuation arclength Newton correction
- shape-preserving interpolation triangular grids
- Muggianu Kohler Redlich-Kister multicomponent extrapolation
- thermodynamic ternary contour reconstruction
- adaptive marching triangles nonlinear scalar field
