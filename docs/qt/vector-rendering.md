# Qt vector rendering

The initial Qt renderer is a custom `QWidget` using `QPainter`, not a scaled
bitmap. It owns no numerical data: it draws a scene assembled from canonical
A/B/C geometry and projection results produced by Rust.

The canonical scene model will be reused for:

- QPainter paths, text, markers, and grids;
- screen-space hit testing after a view transform;
- SVG path/text generation;
- PNG rasterization at the requested resolution; and
- numerical CSV geometry export.

The feasibility canvas already uses antialiased paths, native text, semantic
A/B/C triangle locations, a grid layer, a plot layer, and click-to-composition
conversion. It intentionally uses only sample paths; it must not be confused
with the production projection renderer.

Before promoting it, compare QPainter with QGraphicsScene using dense contour
and point datasets. Preserve crisp zoom, logical line widths, exact A/B/C
mapping, and shared hit/render geometry in either route.