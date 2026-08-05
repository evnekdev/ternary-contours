# Qt Designer UI build pipeline

```text
Qt Designer .ui XML
  -> CMake AUTOUIC invokes Qt uic
  -> generated ui_*.h (build output; never edited)
  -> thin C++ Qt adapter
  <-> framework-neutral Rust reducer and models
```

`apps/ternary-contours-qt/CMakeLists.txt` lists every `.ui` file as a target
source and enables `CMAKE_AUTOUIC`; a change to any form regenerates the
corresponding generated UI class at build time.

In parallel, `ternary-contours-gui-core/build.rs` parses every `.ui` file using
`roxmltree`, rejects invalid XML, duplicate names, generic public names,
non-translatable strings, duplicate properties, missing accessibility metadata, unmanaged `QDialog`/`QWidget` containers, and fixed child geometry. It generates the
Rust `QtUiElementId`, `QtUiElementDefinition`, and inventory in `OUT_DIR`.

Run these checks without a Qt SDK:

```text
cargo test -p ternary-contours-gui-core
cargo run -p ternary-contours-gui-core --bin generate-qt-ui-docs -- --check
```

Qt 6 CMake configuration then verifies the actual `uic` path when the required
SDK is available. CI will add that job on a Qt 6 image rather than silently
substituting Qt 5.