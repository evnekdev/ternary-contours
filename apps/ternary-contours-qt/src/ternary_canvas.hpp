#pragma once

#include <QPointF>
#include <QStringList>
#include <QVector>
#include <QWidget>

class TernaryCanvas final : public QWidget {
    Q_OBJECT
public:
    explicit TernaryCanvas(QWidget* parent = nullptr);
    void setPlotVisible(bool visible);
    void setGridVisible(bool visible);
    void setComponentNames(const QStringList& names);
    void setSourceVertices(const QVector<QPointF>& compositions);
    void fitTriangleToView();
signals:
    void compositionSelected(double a, double b, double c);
protected:
    void mousePressEvent(QMouseEvent* event) override;
    void mouseMoveEvent(QMouseEvent* event) override;
    void mouseReleaseEvent(QMouseEvent* event) override;
    void wheelEvent(QWheelEvent* event) override;
    void paintEvent(QPaintEvent* event) override;
private:
    QPolygonF triangle() const;
    QPointF pointForComposition(double a, double b, double c) const;
    bool compositionForPoint(const QPointF& point, double* a, double* b, double* c) const;
    bool plot_visible_ = true;
    bool grid_visible_ = true;
    QStringList component_names_{QStringLiteral("A"), QStringLiteral("B"), QStringLiteral("C")};
    QVector<QPointF> source_vertices_;
    QPointF selected_composition_{-1.0, -1.0};
    QPointF pan_origin_;
    QPointF pan_;
    bool panning_ = false;
    qreal zoom_ = 1.0;
};