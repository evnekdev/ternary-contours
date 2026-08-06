#pragma once

#include "rust_bridge.hpp"

#include <QMainWindow>
#include <QPoint>
#include <QSet>
#include <QString>
#include <QVector>
#include <cstdint>
#include <memory>
#include <vector>

class GridTableModel;
class CollapsibleSection;
class QStandardItemModel;
class QCloseEvent;
class QModelIndex;
class QLineEdit;

namespace Ui { class MainWindow; }

enum class ViewerWidgetCommand {
    SelectGrid, SelectPhase, SelectProperty, SetInteractionMode,
    SetVertexVisibility, SetRegularGridEdges, SetMarkerSize, SetLabelMode,
    SetLabelDecimals, SetLabelsSelectedOnly,
    SetAutomaticRange, CommitIsoMinimum, CommitIsoMaximum, CommitIsoStep,
    SetSamplingSubdivisions, SetSourceInterpolation, SetCubicMethod,
    SetPartialDomainPolicy, SetContinuation, SetRegularizationEnabled,
    SetRegularizationSpacing, SetPathDisplayMode,
    SetMasterPlotVisible, SetSamplingGridVisible, SetSourceVerticesVisible,
    SetQueryPointsVisible, SetResultsTableVisible, SetStableIsothermsVisible,
    SetStableUnivariantsVisible, SetBinaryInvariantsVisible,
    SetInteriorInvariantsVisible, SetAxisLabelsVisible, SetCornerNamesVisible,
    SetLegendVisible, SetPathVerticesVisible, SetContourEndpointsVisible,
    SetUnivariantEndpointsVisible, SetInvariantIdsVisible, SetUnivariantIdsVisible,
    SetPhasePairLabelsVisible, SetContainingTriangleVisible, SetLineWidth,
    SetPlotMarkerSize, Fit, Reset, RestoreLayout, AddQuery,
    RemoveSelectedQuery, RemoveAllQueries, ResetAutomaticRange,
    SelectVertex, CommitVertexEdit, CommitBulkState,
};

struct ViewerQuery {
    std::uint64_t id = 0;
    std::uint32_t grid_index = 0;
    std::uint32_t phase_id = 0;
    QString property;
    double a = 0.0;
    double b = 0.0;
    double c = 0.0;
    TcqtInspectionResult result{};
};

struct ViewerState {
    std::uint32_t grid_index = 0;
    std::uint32_t phase_id = 0;
    std::uint32_t field_index = 0;
    QString property;
    // 0 = Vertex, 1 = Interpolate.  Keep this semantic order aligned with the
    // Designer combo box and TernaryCanvas interaction routing.
    int interaction_mode = 0;
    bool show_calculated = true;
    bool show_extrapolated = true;
    bool show_cut_off = true;
    bool show_missing = true;
    bool show_regular_grid_edges = true;
    bool show_source_vertices = true;
    bool show_query_points = true;
    bool show_results_table = true;
    bool show_master_plot = true;
    bool show_sampling_grid = true;
    bool show_stable_isotherms = true;
    bool show_stable_univariants = true;
    bool show_binary_invariants = true;
    bool show_interior_invariants = true;
    bool show_axis_labels = true;
    bool show_corner_names = true;
    bool show_legend = false;
    bool show_path_vertices = false;
    bool show_contour_endpoints = false;
    bool show_univariant_endpoints = false;
    bool show_invariant_ids = false;
    bool show_univariant_ids = false;
    bool show_phase_pair_labels = false;
    bool show_containing_triangle = false;
    bool labels_selected_only = false;
    int marker_size = 6;
    int plot_marker_size = 6;
    int line_width = 2;
    int label_mode = 0;
    int label_decimals = 3;
    int path_display_mode = 1;
    QSet<std::uint32_t> selected_rows;
    QVector<ViewerQuery> queries;
    std::uint64_t next_query_id = 1;
    std::uint64_t calculation_generation = 0;
    std::uint64_t options_revision = 0;
    std::uint64_t active_dataset_revision = 0;
    std::uint64_t active_options_revision = 0;
    std::uint64_t active_request_generation = 0;
    bool calculation_running = false;
    bool pending_recalculation = false;
    bool has_last_valid_projection = false;
    bool projection_is_stale = false;
    TcqtViewerCalculationOptions options{true, 0.0, 0.0, 100.0, 20, true, 0.02, 0, 3, 2, 1};
};

class MainWindow final : public QMainWindow {
    Q_OBJECT
public:
    explicit MainWindow(QWidget* parent = nullptr);
    ~MainWindow() override;
protected:
    void closeEvent(QCloseEvent* event) override;
private slots:
    void newDocument();
    void openDocument();
    void saveDocument();
    void saveDocumentAs();
    void exportPng();
    void exportSvg();
    void exportLinesCsv();
    void addGrid(bool regular);
    void removeSelectedGrid();
    void duplicateSelectedGrid();
    void renameSelectedGrid();
    void addPhase();
    void removeSelectedPhase();
    void addProperty();
    void addIrregularRow();
    void runRustCalculation();
    void updateComposition(double a, double b, double c);
    void selectProjectNode(const QModelIndex& index);
    void commitTitle();
    void commitComponentA();
    void commitComponentB();
    void commitComponentC();
    void copyGridSelection();
    void pasteGridClipboard();
    void showGridContextMenu(const QPoint& position);
    void extrapolateSelectedRegularField();
    void extrapolateViewerPhase();
    void extrapolateViewerTargets(const QVector<std::uint32_t>& rows);
    void showViewerMeshExtrapolationDialog(std::uint32_t scope, const QVector<std::uint32_t>& rows = {});
private:
    bool saveToPath(const QString& path);
    bool performSave(bool save_as);
    void commitPendingEditors();
    bool confirmDocumentReplacement(const QString& action);
    void rebuildFromRust(std::uint32_t preferred_grid = 0);
    void rebuildTree();
    void updateActionState();
    void updateViewerActionState();
    void updateWindowTitle();
    void updateDocumentPresentation();
    void reportBridgeStatus(const QString& message, bool success);
    void restoreWindowLayout();
    void saveWindowLayout();
    void dispatchViewerWidgetCommand(ViewerWidgetCommand action);
    void refreshViewerFieldSelectors();
    void refreshViewerVertices();
    void refreshViewerQueries();
    bool refreshProjectionCanvas(bool accept_empty = false);
    void scheduleViewerCalculation();
    void addInterpolationQuery(double a, double b, double c);
    void setInterpolationPreview(const TcqtLocatedPoint& location);
    void clearInterpolationPreview();
    void selectViewerVertex(std::uint32_t row, bool additive);
    void editViewerVertex(std::uint32_t row, const QPoint& global_position);
    void showViewerVertexContextMenu(std::uint32_t row, const QPoint& global_position);
    bool commitViewerNumber(QLineEdit* editor, double* target, const QString& label);
    bool commitViewerCalculationOptions(ViewerWidgetCommand source);
    void syncViewerPanelControls();
    void updateViewerSelectionDetails();
    void setViewerCalculationStatus(const QString& message, bool error = false);

    std::unique_ptr<Ui::MainWindow> ui_;
    QStandardItemModel* tree_model_ = nullptr;
    GridTableModel* grid_model_ = nullptr;
    std::uint32_t selected_grid_ = 0;
    std::uint32_t selected_phase_id_ = 0;
    ViewerState viewer_;
    bool synchronizing_ = false;
    bool editor_commit_failed_ = false;
    std::vector<std::unique_ptr<CollapsibleSection>> viewer_sections_;
};