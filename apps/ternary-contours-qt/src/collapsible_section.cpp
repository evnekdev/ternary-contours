#include "collapsible_section.hpp"

#include <QGroupBox>
#include <QSettings>
#include <QToolButton>
#include <QWidget>

CollapsibleSection::CollapsibleSection(QToolButton* header, QWidget* content,
                                       bool expanded_by_default,
                                       const QString& settings_key, QObject* parent)
    : QObject(parent), content_(content), header_(header),
      expanded_by_default_(expanded_by_default), settings_key_(settings_key) {
    Q_ASSERT(content_ != nullptr);
    Q_ASSERT(header_ != nullptr);
    header_->setCheckable(true);
    header_->setToolButtonStyle(Qt::ToolButtonTextBesideIcon);
    if (auto* const group = qobject_cast<QGroupBox*>(content_)) {
        // The Designer header is the only visible heading for the collapsible
        // group; retain the Designer content and layout below it.
        group->setTitle(QString());
        group->setFlat(true);
    }
    connect(header_, &QToolButton::toggled, this,
            [this](bool expanded) { setExpanded(expanded, true); });
}

void CollapsibleSection::restore() {
    QSettings settings("evnekdev", "ternary-contours-qt");
    setExpanded(settings.value(settings_key_, expanded_by_default_).toBool(), false);
}

void CollapsibleSection::resetToDefault() { setExpanded(expanded_by_default_, true); }

bool CollapsibleSection::isExpanded() const { return header_->isChecked(); }

void CollapsibleSection::setExpanded(bool expanded, bool persist) {
    header_->blockSignals(true);
    header_->setChecked(expanded);
    header_->setArrowType(expanded ? Qt::DownArrow : Qt::RightArrow);
    header_->blockSignals(false);
    content_->setVisible(expanded);
    if (persist) {
        QSettings settings("evnekdev", "ternary-contours-qt");
        settings.setValue(settings_key_, expanded);
    }
}