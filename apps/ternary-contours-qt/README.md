# Ternary Contours Qt feasibility prototype

This directory is intentionally outside the default Cargo build. It is the first
Qt 6 migration prototype; the supported existing CLI and feature-gated egui
viewer remain available during parity work.

The prototype provides a native `QMainWindow`, conventional menus, exactly two
primary tabs (`Data` and `Viewer`), a tree and table view, a status bar, a
native `.tct` Open dialog, a vector `QPainter` ternary canvas, persisted
splitter geometry, and a `QtConcurrent` worker call into a Rust static library.

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

Without `TCQT_RUST_BRIDGE_LIBRARY`, the shell still builds and visibly reports
that its Rust feasibility bridge is unavailable. This fallback is deliberate:
it permits UI and multi-monitor checks independently from linker setup.

See `docs/qt/building.md` and `docs/architecture/qt-ui-decision.md` for the
selected integration boundary, current local SDK limitation, deployment, and
licensing requirements.
## Viewer control wiring

The Qt Viewer uses one `ViewerState` in `MainWindow`; widgets never own a
separate numerical or document copy. Every visible Viewer control follows:

`widget signal → QAction or ViewerAction → Rust bridge/state → snapshot → canvas and widgets`.

| Object | Command | State/effect |
| --- | --- | --- |
| `comboViewerGrid` | `SelectGrid` | Selects the authoritative grid index, refreshes fields, vertices, and queries. |
| `comboViewerPhase` | `SelectPhase` | Selects the stable phase ID and refreshes its fields. |
| `comboViewerProperty` | `SelectProperty` | Selects `(grid, phase ID, property)` and rebuilds its typed source markers. |
| `comboViewerMode` | `SetInteractionMode` | Select, edit, or add a Rust-evaluated interpolation query. |
| `checkViewerCalculated`, `checkViewerNonExisting`, `checkViewerCutOff`, `checkViewerMissing` | `SetVertexFilter` | Applies the typed classified-value filter to the canvas. |
| `spinViewerMarkerSize` | `SetMarkerSize` | Updates the shared canvas marker size. |
| View-menu layer actions | `SetPlotLayer`, `SetGridLayer`, `SetSourceVertices`, `SetQueryPoints`, `SetResultsVisible` | Update shared presentation state and redraw only. |
| Fit/Reset/Restore Layout | `Fit`, `Reset`, `RestoreLayout` | Change only the canvas transform or saved splitter layout. |
| Query-clear actions | `RemoveSelectedQuery`, `RemoveAllQueries` | Update persistent Qt query presentation rows and markers. |
| `buttonRunRustCalculation` | `tcqt_calculate_viewer` | Runs the existing Rust projection pipeline with revision/generation rejection. |

The Rust bridge owns vertex edits, bulk state changes, field interpolation,
projection preparation, undo, dirty revision, and stale-result rejection.