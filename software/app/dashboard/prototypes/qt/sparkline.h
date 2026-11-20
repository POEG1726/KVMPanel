// Framed sparkline (small row chart or the larger multi-series memory chart), painted with QPainter.
#pragma once

#include <QColor>
#include <QPointer>
#include <QQuickPaintedItem>

class PanelClient;

class Sparkline : public QQuickPaintedItem
{
    Q_OBJECT
    Q_PROPERTY(QObject *source READ source WRITE setSource NOTIFY changed)
    Q_PROPERTY(QStringList metrics MEMBER m_metrics WRITE setMetrics NOTIFY changed)
    Q_PROPERTY(QVariantList colors MEMBER m_colors WRITE setColors NOTIFY changed)
    Q_PROPERTY(bool big MEMBER m_big WRITE setBig NOTIFY changed)
    Q_PROPERTY(qreal unit MEMBER m_unit WRITE setUnit NOTIFY changed)

public:
    explicit Sparkline(QQuickItem *parent = nullptr);
    void paint(QPainter *p) override;

    QObject *source() const;
    void setSource(QObject *s);
    void setMetrics(const QStringList &m) { m_metrics = m; emit changed(); update(); }
    void setColors(const QVariantList &c) { m_colors = c; emit changed(); update(); }
    void setBig(bool b) { m_big = b; emit changed(); update(); }
    void setUnit(qreal u) { m_unit = u; emit changed(); update(); }

signals:
    void changed();

private:
    QPointer<PanelClient> m_src;
    QStringList m_metrics;
    QVariantList m_colors;
    bool m_big = false;
    qreal m_unit = 1;   // mockup px -> screen px (for border width / radius)
};
