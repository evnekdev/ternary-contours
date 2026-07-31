# One-Dimensional Alpha Interval

## Canonical representation

For a directed interval from endpoint 0 to endpoint 1, define

\[
t=\frac{x-x_0}{x_1-x_0}, \qquad 0\le t\le1.
\]

The normalized cubic interval is

\[
y(t)=y_0(1-t)+y_1t+(1-t)t(\alpha_0+\alpha_1t).
\]

Important convention:

> `alpha1` multiplies `t`, the coordinate increasing from the first endpoint to the second endpoint.

The endpoint-linear part is stored implicitly through `y0` and `y1`. Only two additional alpha coefficients are needed.

## Relation to local direct cubic coefficients

Let

\[
y=((a\,\Delta x+b)\Delta x+c)\Delta x+d,
\qquad \Delta x=x-x_0,
\]

and let `h = x1-x0`. Then

\[
\alpha_1=-ah^3,
\]

\[
\alpha_0=\alpha_1-bh^2.
\]

Conversely,

\[
a=-\frac{\alpha_1}{h^3},
\]

\[
b=\frac{\alpha_1-\alpha_0}{h^2},
\]

\[
c=\frac{(y_1-y_0)+\alpha_0}{h},
\]

\[
d=y_0.
\]

## Reversing an interval

Reverse the direction so the new parameter is

\[
s=1-t.
\]

The reversed interval has endpoints swapped and alpha coefficients

\[
\alpha'_0=\alpha_0+\alpha_1,
\]

\[
\alpha'_1=-\alpha_1.
\]

This must satisfy

\[
y_{\mathrm{forward}}(t)=y_{\mathrm{reverse}}(1-t).
\]

Centralize this operation in one tested method.

## Centered coordinate

Define

\[
u=t-\frac12.
\]

Then reversing the edge gives

\[
u\mapsto-u.
\]

The alpha polynomial becomes

\[
\alpha_0+\alpha_1t
=\left(\alpha_0+\frac{\alpha_1}{2}\right)+\alpha_1u.
\]

In centered form:

- the centered constant is invariant under direction reversal;
- the coefficient multiplying the odd coordinate changes sign.

This is the conceptual link to symmetric Redlich-Kister/Muggianu-style formulations.

## Required convention tests

Use asymmetric data so incorrect conventions cannot accidentally pass:

- direct evaluation of a known alpha interval;
- direct-cubic/alpha round trip on a non-unit interval;
- equivalence of every `spline1d` direct and alpha API for left, middle, and right intervals;
- reversal identity at several interior points;
- explicit failure test for the incorrect form `alpha0 + alpha1*(1-t)`.
