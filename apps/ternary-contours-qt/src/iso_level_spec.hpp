#pragma once

#include <QLocale>
#include <QString>
#include <QStringList>
#include <QVector>
#include <QRegularExpression>
#include <algorithm>
#include <cmath>

struct ParsedIsoLevelSpec {
    bool has_range = false;
    double minimum = 0.0;
    double maximum = 0.0;
    double step = 100.0;
    QVector<double> levels;
    QString error;
    bool valid() const { return error.isEmpty() && !levels.isEmpty(); }
};

inline bool appendIsoLevel(QVector<double>& levels, const QString& token, QString* error) {
    bool ok = false;
    const auto value = QLocale::c().toDouble(token.trimmed(), &ok);
    if (!ok || !std::isfinite(value)) {
        *error = QStringLiteral("%1 is not a finite number").arg(token.trimmed());
        return false;
    }
    levels.append(value);
    return true;
}

inline ParsedIsoLevelSpec parseIsoLevelSpec(const QString& text) {
    ParsedIsoLevelSpec result;
    const auto input = text.trimmed();
    if (input.isEmpty()) { result.error = QStringLiteral("Enter a range or at least one explicit level."); return result; }
    const auto parts = input.split(';');
    if (parts.size() > 2) { result.error = QStringLiteral("Use one optional ';' between the range and extra levels."); return result; }
    const auto rangeText = parts.value(0).trimmed();
    const auto extraText = parts.size() == 2 ? parts.at(1).trimmed() : QString();
    const auto fields = rangeText.split(QRegularExpression(QStringLiteral("\\s+")), Qt::SkipEmptyParts);
    if (fields.size() == 3) {
        bool okMin = false, okMax = false, okStep = false;
        result.minimum = QLocale::c().toDouble(fields.at(0), &okMin);
        result.maximum = QLocale::c().toDouble(fields.at(1), &okMax);
        result.step = QLocale::c().toDouble(fields.at(2), &okStep);
        if (!okMin || !okMax || !okStep || !std::isfinite(result.minimum) || !std::isfinite(result.maximum) || !std::isfinite(result.step)) result.error = QStringLiteral("Range values must be finite.");
        else if (result.maximum < result.minimum || result.step <= 0.0) result.error = QStringLiteral("Require maximum >= minimum and step > 0.");
        else {
            result.has_range = true;
            for (double value = result.minimum; value <= result.maximum + std::abs(result.step) * 1.0e-12; value += result.step) {
                result.levels.append(std::min(value, result.maximum));
                if (result.levels.size() > 10000) { result.error = QStringLiteral("The range creates too many levels."); break; }
            }
        }
    } else if (parts.size() == 2 && !rangeText.isEmpty()) {
        result.error = QStringLiteral("The range side requires exactly minimum maximum step.");
    } else if (!rangeText.isEmpty()) {
        for (const auto& token : rangeText.split(',', Qt::KeepEmptyParts)) if (token.trimmed().isEmpty() || !appendIsoLevel(result.levels, token, &result.error)) return result;
    }
    if (!result.error.isEmpty()) return result;
    if (!extraText.isEmpty()) {
        for (const auto& token : extraText.split(',', Qt::KeepEmptyParts)) if (token.trimmed().isEmpty() || !appendIsoLevel(result.levels, token, &result.error)) return result;
    } else if (parts.size() == 2 && !result.has_range) {
        result.error = QStringLiteral("A semicolon must be followed by extra levels."); return result;
    }
    if (result.levels.isEmpty()) { result.error = QStringLiteral("Enter a range or at least one explicit level."); return result; }
    std::sort(result.levels.begin(), result.levels.end());
    QVector<double> unique;
    for (const auto value : result.levels) {
        if (unique.isEmpty() || std::abs(value - unique.back()) > 1.0e-12 * std::max({1.0, std::abs(value), std::abs(unique.back())})) unique.append(value);
    }
    result.levels = unique;
    return result;
}
