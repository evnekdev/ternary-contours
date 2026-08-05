[Qt product and architecture contract](product-and-architecture-contract.md) is normative for this document.

# Qt testing plan

## Core

Run reducer, contract, revision, stale-result, document-dirty, and generated
GUI documentation tests in `ternary-contours-gui-core` without Qt.

## Qt models

Exercise models with QtTest using stable object names and test regular/irregular
editable flags, phase IDs, required T, paste transactions, validation roles,
insert/remove signals, and selection preservation.

## Qt shell

Test File/Grid/View/About menu structure, exactly two tabs, shortcuts, native
dialog requests through injected services, status-bar feedback, Enter commits,
layer action synchronization, canvas/query selection, and result-table updates.

## Window/layout/manual matrix

Use native Qt behavior for dragging and maximize/restore. Test QSettings
recovery with negative monitor positions, removed monitors, and 125/150/200%
DPI. Manually verify the Qt 6 prototype on at least two physical/logical
monitors before claiming multi-monitor acceptance. Small layouts must use
splitters, scroll views, or explicit collapse—not outer-window growth.

The current local environment lacks Qt 6, so QtTest and physical monitor checks
are deliberately pending. Rust bridge and core tests remain automated now.