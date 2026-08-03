# Interactive liquidus inspection viewer

The optional native viewer is a manual numerical-inspection surface for a TCT
file. It reuses the exact command-line pipeline:

```text
TCT parser -> dataset validation -> LiquidusProjection -> Plotters / plotters-ternary
```

It does not implement another parser, field evaluator, or stable-topology
algorithm.

## Launch

The viewer is intentionally excluded from default CLI builds. Enable it when it
is needed:

```text
cargo run -p ternary-contours-cli --features viewer -- \
    view tools/ternary-contours-cli/fixtures/interior-invariant.tct

cargo run -p ternary-contours-cli --features viewer -- \
    view data.tct --levels 800:1400:50 --sampling-subdivisions 40
```

Without `--features viewer`, `view` exits with the enabling command while
`inspect`, `validate`, and `plot` remain headless.

## Controls and calculation policy

The toolbar reloads, recalculates, exports SVG/PNG, and fits/resets the image
view. The side panel owns all current options:

- Levels accept `800,900,1000` or `800:1400:50`; invalid text is reported
  before calculation and never crashes the window.
- Sampling subdivisions, regularization, and regularization spacing are applied
  only when **Apply / recalculate** is selected.
- Layer visibility, labels, legend, line width, marker size, and raw versus
  regularized display redraw the shared bitmap without stable-boundary tracing.
- Raw, regularized, and overlay modes cache the raw projection and, when
  enabled, the regularized projection under the same parsed dataset.

Parsing, validation, and calculation run on an owned worker request. Results
carry a generation number; stale results are ignored. A failed reload leaves
the last valid dataset, projection, and texture intact and reports the parser
or numerical diagnostic in the status area.

## Inspection and diagnostics

Scroll over the plot to zoom; drag to pan; fit/reset only crop and scale the
rendered bitmap. Ternary compositions remain unchanged. The viewer centralizes
composition to canonical equilateral logical coordinates, bitmap coordinates,
and screen coordinates for hit testing.

Click priority is invariant node, univariant, isotherm, then source sample. The
selection panel reports available numerical data: phase names and IDs,
composition, temperature, incident paths, point counts, endpoints, path length,
pair residuals, and regularization diagnostics. Stable-contour field residuals
are not currently retained by the numerical result API and are labelled as
such.

The collapsible Diagnostics panel enables path vertices, contour/univariant
endpoints, invariant and univariant IDs, and phase-pair labels. Sampling/source
points are regular render layers. Sampling-grid edge diagnostics are not shown
for irregular source grids because no common edge topology exists there.

## Export and limits

Export uses the same static Plotters configuration as the view and writes
`<input>.viewer.svg` or `<input>.viewer.png` beside the source TCT file.

Zoom/pan is intentionally a viewer-only bitmap transform rather than an
arbitrary Plotters viewport. It preserves the renderer and numerical pipeline,
but vector export always uses the full configured static plot. Diagnostic text
is a viewer overlay; core layers, visibility, styling, and raw/regularized paths
are shared by image and export.

The viewer does not edit TCT tables, read `.xlsx`, invoke external
thermodynamic software, discover isolated closed univariants, trace nonlinear
paths directly, draw stable filled regions or 3D surfaces, or provide web/FFI
integration.

## Manual smoke-test checklist

Use the committed fixtures below on the intended desktop platform:

1. Load `minimal-regular.tct`; confirm initial calculation and layer toggles.
2. Use `different-subdivisions.tct`; change sampling subdivisions and apply.
3. Use `interior-invariant.tct`; select an interior invariant and a univariant.
4. Use `binary-invariants.tct`; select a binary invariant.
5. Use `hidden-metastable-equality.tct`; check raw, regularized, and overlay
   modes after enabling regularization.
6. Use `partial-phase-domain.tct` and `irregular-phase-grids.tct`; enable source
   and sampling-point diagnostics.
7. Edit a valid input, reload it, then make it malformed and reload again;
   confirm the last valid plot remains visible.
8. Export both SVG and PNG and inspect the files.

Manual launch smoke result (Windows desktop, 2026-08-03): passed. The
interior-invariant fixture opened through the `eframe` glow backend and ran
without a startup error; the test process was then closed after launch. Full
visual interaction remains a desktop checklist because the automated suite
intentionally requires no display server.