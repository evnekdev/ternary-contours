#pragma once

#include <QColor>
#include <QString>

#include <cstdint>

enum class ScalarMarkerShape {
    Circle,
    Square,
    Triangle,
};

struct ScalarStateAppearance {
    QColor fill;
    QColor outline;
    ScalarMarkerShape marker;
    bool filled;
    QString short_label;
    QString accessible_name;
};

inline ScalarStateAppearance scalarStateAppearance(std::uint32_t state) {
    switch (state) {
    case 0:
        return {QColor(42, 150, 75), QColor(26, 110, 54), ScalarMarkerShape::Circle, true,
                QStringLiteral("Calculated"), QStringLiteral("Calculated value")};
    case 4:
        return {QColor(62, 95, 196), QColor(43, 66, 144), ScalarMarkerShape::Square, true,
                QStringLiteral("EX"), QStringLiteral("Extrapolated value")};
    case 2:
        return {QColor(215, 115, 35), QColor(165, 75, 20), ScalarMarkerShape::Triangle, true,
                QStringLiteral("CO"), QStringLiteral("Cut-off value")};
    default:
        return {Qt::transparent, QColor(105, 105, 105), ScalarMarkerShape::Circle, false,
                QStringLiteral("NA"), QStringLiteral("Missing value")};
    }
}

inline QString scalarStateTooltip(std::uint32_t state, const QString& note = {}) {
    const auto appearance = scalarStateAppearance(state);
    if (state == 4) return QStringLiteral("Extrapolated value");
    const auto base = appearance.accessible_name;
    return note.isEmpty() ? base : base + QStringLiteral(" - ") + note;
}