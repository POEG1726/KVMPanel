// E2 colour blobs, CPU-only: the three blurred discs are pre-rendered once at 1/4 resolution;
// per tick they are only moved/scaled and screen-blended into a small opaque buffer, which is
// then stretched to the item (blurred content, so the upscale is invisible).
#pragma once

#include <QImage>
#include <QQuickPaintedItem>

class BlobLayer : public QQuickPaintedItem
{
    Q_OBJECT
    Q_PROPERTY(qreal t MEMBER m_t WRITE setT NOTIFY changed)                    // seconds
    Q_PROPERTY(qreal intensity MEMBER m_intensity WRITE setIntensity NOTIFY changed)
    Q_PROPERTY(int downscale MEMBER m_down NOTIFY changed)

public:
    explicit BlobLayer(QQuickItem *parent = nullptr);
    void paint(QPainter *p) override;

    void setT(qreal t) { m_t = t; compose(); }
    void setIntensity(qreal i) { m_intensity = i; compose(); }

signals:
    void changed();

private:
    void compose();

    qreal m_t = 0, m_intensity = 0.55;
    int m_down = 4;
    QImage m_blobs[3];
    QImage m_layer, m_buf;
    QSize m_builtFor;
};
