#pragma once

#include <QColor>
#include <QPointF>
#include <QSet>
#include <QPolygonF>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QWidget>
#include <cstdint>
#include <optional>

struct CanvasVertex {
    QPointF composition;
    std::uint32_t row = 0;
    std::uint32_t state = 3;
    QString label;
};

struct CanvasPath {
    QVector<QPointF> compositions;
    std::uint32_t type = 0;
    // Semantics and style are supplied by Rust; this struct is a QPainter-ready
    // transport object, not a second numerical or styling model.
    std::uint32_t rgba = 0xff2d6eb4;
    double stroke_width = 1.5;
    std::uint32_t marker_kind = 0;
    // 0 raw, 1 regularized; supplied by Rust with the typed record.
    std::uint32_t path_source = 1;
    QString line_id;
    QString phase_pair;
};

struct CanvasInterpolationPreview {
    QPointF composition;
    QPolygonF containing_triangle;
    QSet<std::uint32_t> source_rows;
};
struct CanvasQuery {
    std::uint64_t id = 0;
    QPointF composition;
    std::uint32_t state = 0;
    bool selected = false;
    QPolygonF containing_triangle;
};

class TernaryCanvas final : public QWidget {
    Q_OBJECT
public:
    explicit TernaryCanvas(QWidget* parent = nullptr);
    void setPlotVisible(bool visible);
    void setGridVisible(bool visible);
    void setSourceVerticesVisible(bool visible);
    void setQueryPointsVisible(bool visible);
    void setComponentNamesVisible(bool visible);
    void setAxisLabelsVisible(bool visible);
    void setLegendVisible(bool visible);
    void setComponentNames(const QStringList& names);
    void setSourceVertices(const QVector<QPointF>& compositions);
    void setInspectionVertices(const QVector<CanvasVertex>& vertices);
    void setProjectionPaths(const QVector<CanvasPath>& paths);
    void setProjectionVisibility(bool master, bool isotherms, bool univariants,
                                 bool binary_invariants, bool interior_invariants);
    void setProjectionPathDisplayMode(int mode);
    void setProjectionAppearance(int line_width, int invariant_marker_size);
    void setDiagnosticVisibility(bool path_vertices, bool contour_endpoints,
                                 bool univariant_endpoints, bool invariant_ids,
                                 bool univariant_ids, bool phase_pair_labels);
    void setVertexLabelSettings(int mode, int decimals, bool selected_only);
    void setQueries(const QVector<CanvasQuery>& queries);
    void setInterpolationPreview(const std::optional<CanvasInterpolationPreview>& preview);
    void setContainingTriangleVisible(bool visible);
    void setMarkerSize(int size);
    void setVertexVisibility(bool calculated, bool extrapolated, bool cut_off, bool missing);
    void setInteractionMode(int mode);
    void setSelectedRows(const QSet<std::uint32_t>& rows);
    void fitTriangleToView();
    void resetView();
signals:
    void compositionSelected(double a, double b, double c);
    void vertexSelected(std::uint32_t row, bool additive);
    void vertexDoubleClicked(std::uint32_t row, QPoint global_position);
    void vertexContextRequested(std::uint32_t row, QPoint global_position);
    void interpolationRequested(double a, double b, double c);
protected:
    void mousePressEvent(QMouseEvent* event) override;
    void mouseDoubleClickEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void wheelEvent(QWheelEvent* event) override;
    void paintEvent(QPaintEvent* event) override;
private:
    QPolygonF triangle() const;
    QPointF pointForComposition(double a, double b, double c) const;
    bool compositionForPoint(const QPointF& point, double* a, double* b, double* c) const;
    std::optional<std::uint32_t> vertexAt(const QPointF& point) const;
    QColor colorForState(std::uint32_t state) const;
    bool visibleState(std::uint32_t state) const;
    bool plot_visible_ = true;
    bool grid_visible_ = true;
    bool source_vertices_visible_ = true;
    bool query_points_visible_ = true;
    bool component_names_visible_ = true;
    bool axis_labels_visible_ = true;
    bool legend_visible_ = false;
    bool show_isotherms_ = true;
    bool show_univariants_ = true;
    bool show_binary_invariants_ = true;
    bool show_interior_invariants_ = true;
    bool show_path_vertices_ = false;
    bool show_contour_endpoints_ = false;
    bool show_univariant_endpoints_ = false;
    bool show_invariant_ids_ = false;
    bool show_univariant_ids_ = false;
    bool show_phase_pair_labels_ = false;
    bool show_containing_triangle_ = false;
    // 0 raw, 1 regularized, 2 overlay.
    int path_display_mode_ = 1;
    bool labels_selected_only_ = false;
    int label_mode_ = 0;
    int label_decimals_ = 3;
    int line_width_ = 2;
    int invariant_marker_size_ = 6;
    bool show_calculated_ = true;
    bool show_extrapolated_ = true;
    bool show_cut_off_ = true;
    bool show_missing_ = true;
    int marker_size_ = 6;
    int interaction_mode_ = 0;
    QStringList component_names_{QStringLiteral("A"), QStringLiteral("B"), QStringLiteral("C")};
    QVector<QPointF> source_vertices_;
    QVector<CanvasVertex> inspection_vertices_;
    QVector<CanvasPath> projection_paths_;
    QVector<CanvasQuery> queries_;
    std::optional<CanvasInterpolationPreview> interpolation_preview_;
    QSet<std::uint32_t> selected_rows_;
    QPointF selected_composition_{-1.0, -1.0};
    QPointF pan_origin_;
    QPointF pan_;
    bool panning_ = false;
    qreal zoom_ = 1.0;
};