[Qt product and architecture contract](product-and-architecture-contract.md) is normative for this document.

# Qt application structure

```text
apps/ternary-contours-qt/
  CMakeLists.txt                 Qt 6 Widgets shell (not default Cargo build)
  src/                           QMainWindow, vector canvas, menu/status wiring
  include/rust_bridge.hpp        narrow Rust bridge ABI
  rust-bridge/                   Rust static library for feasibility work

tools/ternary-contours-gui-core/
  src/lib.rs                     toolkit-free actions, effects, reducer, revisions,
                                 contracts, state documentation generator
  src/bin/generate_gui_contract_docs.rs

tools/ternary-contours-cli/
  src/viewer/contract.rs         legacy egui compatibility and migration audits
```

The Qt UI dispatches `UiAction` values to the core. The core returns `UiEffect`
values. A Qt effect executor performs native dialogs, file I/O, clipboard work,
worker submission, and scene/model updates, then sends a typed completion action
back to the core. Workers never access Qt widgets or models.

The first Qt shell has exactly two primary tabs: **Data** and **Viewer**. The
legacy egui viewer retains its existing tabs temporarily and is not the target
information architecture.