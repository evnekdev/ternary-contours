#pragma once

#include <QObject>

class QToolButton;
class QWidget;

// Supplies common collapse/expand behavior for Designer-declared headers and
// Designer-owned content. It never re-parents or rebuilds the widget tree.
class CollapsibleSection final : public QObject {
    Q_OBJECT
public:
    explicit CollapsibleSection(QToolButton* header, QWidget* content,
                                bool expanded_by_default,
                                const QString& settings_key, QObject* parent = nullptr);

    void restore();
    void resetToDefault();
    bool isExpanded() const;

private:
    void setExpanded(bool expanded, bool persist);

    QWidget* content_ = nullptr;
    QToolButton* header_ = nullptr;
    bool expanded_by_default_ = false;
    QString settings_key_;
};