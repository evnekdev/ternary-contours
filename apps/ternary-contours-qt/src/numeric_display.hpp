#pragma once

#include <QLocale>
#include <QString>
#include <QStringList>

// The only sanctioned formatter for floating-point values that cross into the
// Qt presentation layer. It is deliberately display-only; no numerical input
// or serialization should call these helpers.
enum class DisplayNumberKind { Temperature, Composition, Property };

inline int displayDecimals(DisplayNumberKind kind) {
    switch (kind) {
    case DisplayNumberKind::Temperature: return 2;
    case DisplayNumberKind::Composition: return 5;
    case DisplayNumberKind::Property: return 3;
    }
    return 3;
}

inline QString displayNumber(double value, DisplayNumberKind kind) {
    // presentation-format: allow - this is the centralized formatter itself.
    return QLocale::c().toString(value, 'f', displayDecimals(kind)); // presentation-format: allow
}

inline QString displayTemperature(double value, const QString& unit = QStringLiteral("\u00B0C")) {
    const auto number = displayNumber(value, DisplayNumberKind::Temperature);
    return unit.isEmpty() ? number : number + QStringLiteral(" ") + unit;
}

inline QString displayComposition(double a, double b, double c) {
    return QStringLiteral("%1, %2, %3")
        .arg(displayNumber(a, DisplayNumberKind::Composition))
        .arg(displayNumber(b, DisplayNumberKind::Composition))
        .arg(displayNumber(c, DisplayNumberKind::Composition));
}

inline QString displayPropertyValue(double value, const QString& property = {}, const QString& unit = {}) {
    const auto number = displayNumber(value, DisplayNumberKind::Property);
    if (property.isEmpty()) return unit.isEmpty() ? number : number + QStringLiteral(" ") + unit;
    auto result = property + QStringLiteral(" = ") + number;
    if (!unit.isEmpty()) result += QStringLiteral(" ") + unit;
    return result;
}
