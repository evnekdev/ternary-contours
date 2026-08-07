# Ternary Contours Qt application

This directory is intentionally outside the default Cargo build.  It is the
Qt 6 desktop application while the established CLI remains supported.

The application has two primary tabs: `Data` and `Viewer`.  The Viewer uses a
Designer-owned split layout: vertically stacked **Vertex inspection** and
**Iso-plots** controls on the left; the ternary canvas above independently
resizable **Interpolation results** and **Invariant points** tables on the right.

## Build

Build the Rust bridge first:

```text
cargo build -p ternary-contours-qt-bridge --release
```

Then configure against a Qt 6 SDK:

```text
cmake -S apps/ternary-contours-qt -B build/qt \
  -DQt6_DIR=/path/to/Qt/6.x/<toolchain>/lib/cmake/Qt6 \
  -DTCQT_RUST_BRIDGE_LIBRARY=/absolute/path/to/ternary_contours_qt_bridge.lib
cmake --build build/qt --config Release
```

See [building.md](../../docs/qt/building.md) for the supported Windows build
script and deployment details.

## Ownership boundary

Rust is authoritative for:

- the TCT document, typed grid cells, classified values, dirty revisions and undo;
- viewer calculation configuration and validation;
- field interpolation, topology, projection preparation and result records;
- semantic plot-scene metadata: calculated layer, colour, stroke width and marker kind.

Qt/C++ owns only native widgets, menus, dialogs, splitter persistence, input
translation, and final `QPainter` execution.  `QPainter` calls must stay in C++
because they require a live Qt paint device; C++ does not choose numerical
methods, mutate document data directly, or derive plot semantics.

## Viewer control wiring

Every visible Viewer control dispatches through `dispatchViewerWidgetCommand`,
then either changes render-only presentation state or commits a Rust-owned
configuration through `tcqt_set_viewer_calculation_options`. A calculation
uses a value-copied Rust option snapshot with
`tcqt_calculate_viewer(options, revision, generation)`; completion is accepted
only for the matching document revision and generation.

| Widget or command | Adapter command | Authoritative state/effect |
| --- | --- | --- |
| `comboViewerGrid`, `comboViewerPhase`, `comboViewerProperty` | `SelectGrid`, `SelectPhase`, `SelectProperty` | Stable grid/phase/property field identity; refreshes source markers and queries. |
| `comboViewerMode` | `SetInteractionMode` | Vertex or Interpolate canvas interaction. |
| Vertex-state, edge, marker and label controls | `SetVertexVisibility`, `SetRegularGridEdges`, `SetMarkerSize`, `SetLabelMode`, `SetLabelDecimals`, `SetLabelsSelectedOnly` | Render-only source-vertex presentation. |
| Iso range, sampling, interpolation, cubic method, fallback, continuation, regularization and path controls | corresponding `Commit...` or `Set...` command | Rust `TcqtViewerCalculationOptions`; schedules a revision-checked calculation. |
| View-menu and layer checkboxes | distinct `SetMasterPlotVisible`, `SetStable...`, `Set...Invariants`, `SetSourceVerticesVisible`, and related commands | Shared render-only visibility state; menu and panel remain synchronized. |
| Fit, Reset, Restore Layout | `Fit`, `Reset`, `RestoreLayout` | Canvas transform or splitter visibility/geometry only. |
| Interpolation results commands | `actionViewerCopyQueries`, `RemoveSelectedQuery`, `RemoveAllQueries` | Transient query TSV export/removal by stable query ID; never changes the TCT document. |
| Invariant points table | `tcqt_invariant_point_count`, `tcqt_invariant_point_at` | Read-only stable-boundary graph nodes from one accepted Rust projection snapshot. |
| Vertex popup and bulk context actions | bridge vertex/bulk mutation | One Rust mutation/transaction updates undo, revision, dirty presentation, Data, Viewer and automatic calculation. |

The source test `visible_qt_viewer_controls_use_the_thin_adapter_and_rust_bridge`
checks the generated UI inventory, action bindings, distinct calculated-layer
commands, Rust option APIs, and the absence of the obsolete no-argument
calculation entry point.

## Isotherm levels and CSV export

The Viewer has one editable **Levels** field. It uses the typed grammar
`minimum maximum step`, a comma-separated manual list, or a range followed by
`;` and extra levels. `AutoDerived` display is never a magic word: after a
stable topology is accepted it writes the invariant-derived `minimum maximum
100` specification. A user commit is `UserEdited`, remains authoritative
across topology changes, and uses the Rust level-only recalculation path.

**Export CSV...** is a modal, revision-pinned workflow. Qt supplies only the
path and three content selections; the Rust bridge serializes the accepted
projection snapshot into seven thermodynamic geometry columns at full
round-trip precision, with atomic file replacement. CSV formatting is data
serialization and must not use the GUI fixed-decimal formatter.
