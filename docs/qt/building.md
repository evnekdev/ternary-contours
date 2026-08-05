# Building the Qt prototype

The Qt application is opt-in during migration; ordinary workspace builds do not
need Qt and continue to build the CLI and legacy egui viewer.

## Prerequisites

- Rust 1.97.1 or newer.
- CMake 3.24 or newer and a C++20 compiler compatible with the Qt SDK.
- Qt **6.5 or newer** with Core, Gui, Widgets, and Concurrent. SVG is needed
  once the export stage is added.
- On Windows, use one compiler family consistently for Qt, the C++ shell, and
  the Rust `staticlib` link step (for example MSVC x64).

CXX-Qt's setup guide also requires `qmake` discoverability (or `QMAKE`) for its
build tooling. The prototype itself uses CMake's `Qt6_DIR` lookup, but later
CXX bridge work will need the same Qt discovery discipline.

## Windows

```text
cargo build -p ternary-contours-qt-bridge --release
cmake -S apps/ternary-contours-qt -B build/qt -G Ninja ^
  -DQt6_DIR=C:/Qt/6.x/msvc2022_64/lib/cmake/Qt6 ^
  -DTCQT_RUST_BRIDGE_LIBRARY=%CD%/target/release/ternary_contours_qt_bridge.lib
cmake --build build/qt --config Release
build/qt/ternary-contours-qt.exe
```

Do not point this at a Qt 5 installation. The CMake project deliberately
requires `Qt6`.

## Linux

```text
cargo build -p ternary-contours-qt-bridge --release
cmake -S apps/ternary-contours-qt -B build/qt -G Ninja \
  -DQt6_DIR=/path/to/Qt/6.x/gcc_64/lib/cmake/Qt6 \
  -DTCQT_RUST_BRIDGE_LIBRARY=$PWD/target/release/libternary_contours_qt_bridge.a
cmake --build build/qt
./build/qt/ternary-contours-qt
```

Run the prototype once with and once without `TCQT_RUST_BRIDGE_LIBRARY`; the
latter should display an explicit bridge-unavailable status instead of failing
or calling Rust from the GUI thread.