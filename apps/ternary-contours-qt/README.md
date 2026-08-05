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