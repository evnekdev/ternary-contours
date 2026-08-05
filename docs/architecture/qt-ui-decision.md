# Qt desktop UI decision

Status: accepted for the migration prototype (2026-08-05).

## Decision

Use a **Qt 6 Widgets `QMainWindow` shell with a narrow C++/Rust bridge**. The
shell owns native widgets, menus, dialogs, models, scene objects, and queued
worker delivery. Rust owns the framework-neutral `ternary-contours-gui-core`
reducer, document/projection revisions, numerical pipeline, and TCT data.

The production bridge will use the maintained CXX family (`cxx` first;
CXX-Qt only where generated `QObject` support is genuinely useful). The
prototype deliberately starts with a small C-compatible `staticlib` ABI so its
threading and deployment boundary are visible and independently testable.
This avoids placing business logic in Qt signal handlers.

## Options investigated

| Approach | Result | Reason |
| --- | --- | --- |
| CXX-Qt + Qt Quick/QML | Not selected for the primary shell | CXX-Qt is actively documented and tested on Linux, Windows, and macOS; its documented happy path is QML. Reproducing a conventional engineering application in QML would add a styling/model risk. |
| Qt 6 Widgets + thin C++ shell + Rust core | **Selected** | `QMainWindow`, `QMenuBar`, `QTabWidget`, `QTreeView`, `QTableView`, `QSplitter`, `QStatusBar`, and `QFileDialog` directly match the required desktop workflow. Qt model/view keeps data outside widgets, matching the reducer/model boundary. |
| Legacy direct Rust Qt bindings | Rejected | They do not provide an equally well-supported Qt 6 Widgets, model/view, and build/deployment path. |

CXX-Qt explicitly supports bridging normal Qt and Rust code through CXX rather
than attempting one-to-one Qt bindings, and documents CI coverage on the three
target desktop platforms. Its installation guide requires a C++ compiler,
CMake 3.24+, Rust, and Qt 5 or 6. Qt's own `QMainWindow` documentation
explicitly supports the menu, toolbar, status bar, and central-widget structure
needed here. Sources: <https://kdab.github.io/cxx-qt/book/>,
<https://kdab.github.io/cxx-qt/book/getting-started/>, and
<https://doc.qt.io/qt-6/qmainwindow.html>.

## Prototype evidence and local limitation

`apps/ternary-contours-qt` contains a Qt 6 Widgets prototype with two tabs,
tree/table views, a native Open dialog, a high-DPI `QPainter` canvas, status
bar, persisted splitters, and `QtConcurrent` worker completion to the GUI
thread. The Rust bridge uses the real `RegularTernaryGrid` constructor plus the
framework-neutral reducer to produce a revisioned calculation request.

This workstation currently has Qt **5.15.2** and no Qt 6 SDK, so Qt 6 CMake
configuration and mixed-DPI manual tests are intentionally blocked rather than
silently validated against the wrong major version. The required next setup
step is documented in `docs/qt/building.md`.

## Consequences

- The old egui viewer remains supported during migration.
- `ternary-contours-gui-core` is toolkit-free and is now the sole owner of
  typed actions, effects, revisions, contracts, and generated GUI documents.
- Qt widgets must dispatch typed actions; they may not mutate document or
  numerical state directly.
- QPainter is the initial vector-rendering candidate. A later benchmark may
  select `QGraphicsScene` only if it improves dynamic scene management without
  reducing coordinate or hit-test consistency.
- The final supported baseline is Qt 6.5+ (Qt Widgets, Concurrent, SVG/Core
  deployment support), CMake 3.24+, and Rust 1.97.1.