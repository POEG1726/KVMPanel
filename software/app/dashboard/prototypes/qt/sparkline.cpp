#include "sparkline.h"
#include "panelclient.h"

#include <QPainter>
#include <QPainterPath>
#include <cmath>

Sparkline::Sparkline(QQuickItem *parent) : QQuickPaintedItem(parent)
{
    setAntialiasing(true);
    connect(this, &QQuickItem::widthChanged, this, [this] { update(); });
    connect(this, &QQuickItem::heightChanged, this, [this] { update(); });
}

QObject *Sparkline::source() const { return m_src; }

void Sparkline::setSource(QObject *s)
{
    if (m_src) disconnect(m_src, nullptr, this, nullptr);
    m_src = qobject_cast<PanelClient *>(s);
    if (m_src) connect(m_src, &PanelClient::updated, this, [this] { update(); });
    emit changed();
    update();
}

void Sparkline::paint(QPainter *p)
{
    p->setRenderHint(QPainter::Antialiasing);
    const qreal bw = qMax<qreal>(1.0, std::round(m_unit * 0.8));
    const QRectF box = QRectF(0, 0, width(), height()).adjusted(bw / 2, bw / 2, -bw / 2, -bw / 2);
    p->setPen(QPen(QColor(255, 255, 255, 33), bw));
    p->setBrush(QColor(0, 0, 0, 89));
    p->drawRoundedRect(box, (m_big ? 4 : 3) * m_unit, (m_big ? 4 : 3) * m_unit);

    // Same viewBox geometry as the SVG in e2-v5.html, stretched to the content box.
    const QRectF in = QRectF(0, 0, width(), height()).adjusted(bw, bw, -bw, -bw);
    const qreal vw = m_big ? 200 : 84, vh = m_big ? 58 : 18;
    const qreal sx = in.width() / vw, sy = in.height() / vh;
    auto X = [&](qreal x) { return in.left() + x * sx; };
    auto Y = [&](qreal y) { return in.top() + y * sy; };

    p->setPen(QPen(QColor(255, 255, 255, 18), (m_big ? 0.6 : 0.5) * sy));
    if (m_big) { for (qreal y = 11.6; y < 58; y += 11.6) p->drawLine(QPointF(X(0), Y(y)), QPointF(X(vw), Y(y))); }
    else { p->drawLine(QPointF(X(0), Y(6)), QPointF(X(vw), Y(6))); p->drawLine(QPointF(X(0), Y(12)), QPointF(X(vw), Y(12))); }

    if (!m_src) return;
    const qreal yb = m_big ? 55 : 16, span = m_big ? 52 : 14, base = m_big ? 57 : 17;
    const int N = PanelClient::HistoryLen;
    const qreal step = (vw - 2) / (N - 1);

    for (int s = 0; s < m_metrics.size(); ++s) {
        const PanelClient::Metric *m = m_src->find(m_metrics[s]);
        if (!m || m->hist.isEmpty()) continue;
        const QColor c = s < m_colors.size() ? m_colors[s].value<QColor>() : QColor("#5ec4ff");
        const int n = m->hist.size();
        const qreal range = m->max - m->min == 0 ? 1 : m->max - m->min;

        // Newest sample at the right edge; null samples break the line.
        QVector<QVector<QPointF>> runs(1);
        for (int i = 0; i < n; ++i) {
            const double v = m->hist[i];
            if (std::isnan(v)) { if (!runs.last().isEmpty()) runs.append(QVector<QPointF>()); continue; }
            const qreal x = (vw - 1) - (n - 1 - i) * step;
            const qreal y = yb - qBound(0.0, (v - m->min) / range, 1.0) * span;
            runs.last().append(QPointF(X(x), Y(y)));
        }
        QColor fill = c;
        fill.setAlpha(m_big ? 0x1f : 0x2e);
        for (const auto &r : runs) {
            if (r.size() < 2) continue;
            QPolygonF poly(r);
            poly.prepend(QPointF(r.first().x(), Y(base)));
            poly.append(QPointF(r.last().x(), Y(base)));
            p->setPen(Qt::NoPen);
            p->setBrush(fill);
            p->drawPolygon(poly);
            p->setPen(QPen(c, (m_big ? 1.3 : 1.1) * sy, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
            p->setBrush(Qt::NoBrush);
            p->drawPolyline(r.constData(), r.size());
        }
    }
}
