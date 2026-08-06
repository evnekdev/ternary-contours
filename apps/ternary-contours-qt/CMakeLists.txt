cmake_minimum_required(VERSION 3.24)
project(ternary-contours-qt VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTOUIC ON)
set(CMAKE_AUTORCC ON)

find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui Widgets Concurrent)

set(TCQT_UI_FILES
    ui/main_window.ui
    ui/settings_dialog.ui
    ui/add_grid_dialog.ui
    ui/interpolation_point_dialog.ui
    ui/phase_editor_dialog.ui
    ui/property_editor_dialog.ui
    ui/about_dialog.ui
)

add_executable(ternary-contours-qt
    src/main.cpp
    src/main_window.cpp
    src/main_window.hpp
    src/collapsible_section.cpp
    src/collapsible_section.hpp
    src/ternary_canvas.cpp
    src/ternary_canvas.hpp
    src/interpolation_point_dialog.cpp
    src/interpolation_point_dialog.hpp
    src/grid_table_model.cpp
    src/grid_table_model.hpp
    include/rust_bridge.hpp
    ${TCQT_UI_FILES}
)

set_target_properties(ternary-contours-qt PROPERTIES
    AUTOUIC_SEARCH_PATHS "${CMAKE_CURRENT_SOURCE_DIR}/ui"
)

target_link_libraries(ternary-contours-qt PRIVATE
    Qt6::Core
    Qt6::Gui
    Qt6::Widgets
    Qt6::Concurrent
)

target_include_directories(ternary-contours-qt PRIVATE include src)

# Build the Rust static library separately, then pass its absolute path here:
#   cmake -S apps/ternary-contours-qt -B build/qt \
#     -DTCQT_RUST_BRIDGE_LIBRARY=/absolute/path/to/ternary_contours_qt_bridge.lib
set(TCQT_RUST_BRIDGE_LIBRARY "" CACHE FILEPATH "Rust static library for the feasibility bridge")
if(TCQT_RUST_BRIDGE_LIBRARY)
    target_link_libraries(ternary-contours-qt PRIVATE "${TCQT_RUST_BRIDGE_LIBRARY}")
    target_compile_definitions(ternary-contours-qt PRIVATE TCQT_WITH_RUST_BRIDGE=1)
endif()
if(WIN32)
    target_link_libraries(ternary-contours-qt PRIVATE
        ws2_32
        ntdll
    )
endif()
