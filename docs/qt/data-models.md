[Qt product and architecture contract](product-and-architecture-contract.md) is normative for this document.

# Qt data models

The Qt application will use Qt model/view classes, not independent text widgets
or re-created widget trees. Qt documents that views operate on external models,
which avoids duplicate widget/data state and enables normal `QTreeView` and
`QTableView` editing, selection, roles, and testing.

| Model | Qt base | Owns/presents | Key rules |
| --- | --- | --- | --- |
| `ProjectTreeModel` | `QAbstractItemModel` | title, corners, phases, properties, grids, fields | Stable project IDs; phase displays `[id] name`; tree selection routes through typed action. |
| `GridTableModel` | `QAbstractTableModel` | A/B/C plus `(phase, property)` cells | Regular A/B/C selectable/copyable but not editable; irregular A/B/C editable after validation. |
| `InterpolationResultsModel` | `QAbstractTableModel` | persistent semantic A/B/C query results | Rows retain query ID/order and update in place on settings change. |
| `ViewerLayerModel` | simple state adapter | Plot/Grid and optional layer visibility | Menu and toolbar actions reflect one shared state. |
| `PhaseListModel` / `PropertyListModel` | `QAbstractListModel` | stable selector options | Names do not replace phase/property IDs. |

`flags()` controls editability; `data()` supplies Display, Edit, ToolTip,
Foreground, Background, and validation roles; accepted commits call the shared
reducer and emit only targeted Qt data-change signals. A QTableView's Enter
commit must not overwrite the previous typed model value until validation has
succeeded.

Reference: <https://doc.qt.io/qt-6/modelview.html>.