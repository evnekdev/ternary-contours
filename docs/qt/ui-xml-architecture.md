[Qt product and architecture contract](product-and-architecture-contract.md) is normative for this document.

# Qt Designer XML architecture

Qt Widgets Designer `.ui` XML is the authoritative static definition for the
Qt application. The UI is loaded once through generated `uic` classes; runtime
updates attach models, change action state, update the custom canvas, and show
status—not rebuild the widget hierarchy.

## Ownership

| Owner | Responsibility |
| --- | --- |
| `.ui` XML | static hierarchy, menu/tab/layout structure, object names, accessibility metadata, size policies, custom-widget placement, and shortcuts. |
| Rust GUI core | semantic contract identity, public state, actions, effects, revisions, transitions, invalidation, hazards, and documentation. |
| Rust models | tree/table/query data, validation, editability, classified values, and numerical state. |
| Qt adapter | signal/slot wiring, model attachment, Qt dialogs/clipboard/window integration, and queued effect completions. |
| Custom widget | vector scene, transforms, hit tests, pan/zoom, and query interaction. |

`main_window.ui` defines the actual main window: File/Grid/View/About menus,
Data and Viewer tabs, their explicit keyboard tab order, splitters, project tree, grid table, results table,
status bar, and the `TernaryCanvas` Designer placeholder. The dialog files
define static settings, add-grid, interpolation-coordinate, phase, property, and About shells. No TCT
parser, numerical interpolation, dirty-state decision, or stale-result rule is
implemented in XML or handwritten Qt layout code.

Every public object has a descriptive `objectName`, `accessibleName`, and
`accessibleDescription` where Qt supports them. `QAction` exposes stable object
and translatable text/status metadata; its semantic accessibility is supplied
by the owning menu/action contract.