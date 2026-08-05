#include "ternary_canvas.hpp"

#include <QMouseEvent>
#include <QPainter>
#include <QPainterPath>
#include <algorithm>
#include <cmath>

namespace {
QPolygonF triangleFor(const QRectF& bounds) {
    const auto side = std::min(bounds.width() * 0.88, bounds.height() * 0.84);
    const auto height = side * std::sqrt(3.0) / 2.0;
    const auto center = bounds.center();
    return {
        QPointF(center.x(), center.y() - height / 2.0),
        QPointF(center.x() - side / 2.0, center.y() + height / 2.0),
        QPointF(center.x() + side / 2.0, center.y() + height / 2.0),
    };
}
}

TernaryCanvas::TernaryCanvas(QWidget* parent) : QWidget(parent) {
    setAccessibleName("Unified ternary viewer");
    setMinimumSize(280, 220);
    setMouseTracking(true);
}

void TernaryCanvas::setPlotVisible(bool visible) {
    plot_visible_ = visible;
    update();
}

void TernaryCanvas::setGridVisible(bool visible) {
    grid_visible_ = visible;
    update();
}

QPointF TernaryCanvas::pointForComposition(double a, double b, double c) const {
    const auto triangle = triangleFor(rect());
    return triangle[0] * a + triangle[1] * b + triangle[2] * c;
}

void TernaryCanvas::mousePressEvent(QMouseEvent* event) {
    const auto triangle = triangleFor(rect());
    const auto point = event->position();
    const auto denominator = (triangle[1].y() - triangle[2].y()) * (triangle[0].x() - triangle[2].x())
        + (triangle[2].x() - triangle[1].x()) * (triangle[0].y() - triangle[2].y());
    if (std::abs(denominator) < 1e-9) return;
    const auto a = ((triangle[1].y() - triangle[2].y()) * (point.x() - triangle[2].x())
        + (triangle[2].x() - triangle[1].x()) * (point.y() - triangle[2].y())) / denominator;
    const auto b = ((triangle[2].y() - triangle[0].y()) * (point.x() - triangle[2].x())
        + (triangle[0].x() - triangle[2].x()) * (point.y() - triangle[2].y())) / denominator;
    const auto c = 1.0 - a - b;
    if (a >= -1e-9 && b >= -1e-9 && c >= -1e-9) {
        selected_composition_ = QPointF(a, b);
        emit compositionSelected(a, b, c);
        update();
    }
}

void TernaryCanvas::paintEvent(QPaintEvent*) {
    QPainter painter(this);
    painter.setRenderHint(QPainter::Antialiasing, true);
    painter.fillRect(rect(), palette().base());
    const auto triangle = triangleFor(rect());
    QPainterPath path;
    path.addPolygon(triangle);
    painter.setPen(QPen(palette().text().color(), 1.5));
    painter.setBrush(Qt::NoBrush);
    painter.drawPath(path);

    if (grid_visible_) {
        painter.setPen(QPen(palette().mid().color(), 0.75, Qt::DashLine));
        for (int step = 1; step < 10; ++step) {
            const auto t = step / 10.0;
            painter.drawLine(pointForComposition(t, 1.0 - t, 0.0), pointForComposition(t, 0.0, 1.0 - t));
            painter.drawLine(pointForComposition(1.0 - t, t, 0.0), pointForComposition(0.0, t, 1.0 - t));
            painter.drawLine(pointForComposition(1.0 - t, 0.0, t), pointForComposition(0.0, 1.0 - t, t));
        }
    }
    if (plot_visible_) {
        painter.setPen(QPen(QColor(40, 120, 210), 2.25, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
        QPainterPath contour;
        contour.moveTo(pointForComposition(0.72, 0.22, 0.06));
        contour.cubicTo(pointForComposition(0.55, 0.35, 0.10), pointForComposition(0.34, 0.45, 0.21), pointForComposition(0.14, 0.48, 0.38));
        painter.drawPath(contour);
    }
    painter.setPen(palette().text().color());
    painter.drawText(triangle[0] + QPointF(-8, -8), "A");
    painter.drawText(triangle[1] + QPointF(-18, 20), "B");
    painter.drawText(triangle[2] + QPointF(10, 20), "C");
    if (selected_composition_.x() >= 0.0) {
        painter.setPen(QPen(QColor(200, 60, 40), 2));
        painter.setBrush(QColor(255, 220, 210));
        painter.drawEllipse(pointForComposition(selected_composition_.x(), selected_composition_.y(), 1.0 - selected_composition_.x() - selected_composition_.y()), 5, 5);
    }
}