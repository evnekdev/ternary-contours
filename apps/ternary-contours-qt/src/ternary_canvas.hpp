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
};

struct CanvasQuery {
    std::uint64_t id = 0;
    QPointF composition;
    std::uint32_t state = 0;
    bool selected = false;
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
    void setQueries(const QVector<CanvasQuery>& queries);
    void setMarkerSize(int size);
    void setVertexVisibility(bool calculated, bool non_existing, bool cut_off, bool missing);
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
    bool show_calculated_ = true;
    bool show_non_existing_ = true;
    bool show_cut_off_ = true;
    bool show_missing_ = true;
    int marker_size_ = 6;
    int interaction_mode_ = 0;
    QStringList component_names_{QStringLiteral("A"), QStringLiteral("B"), QStringLiteral("C")};
    QVector<QPointF> source_vertices_;
    QVector<CanvasVertex> inspection_vertices_;
    QVector<CanvasPath> projection_paths_;
    QVector<CanvasQuery> queries_;
    QSet<std::uint32_t> selected_rows_;
    QPointF selected_composition_{-1.0, -1.0};
    QPointF pan_origin_;
    QPointF pan_;
    bool panning_ = false;
    qreal zoom_ = 1.0;
};