#include "bloblayer.h"

#include <QPainter>
#include <QVector>
#include <cmath>

namespace {

// Running-sum box blur along one axis. step = distance between samples, stride = distance between lines.
void boxBlur(const float *src, float *dst, int n, int r, int step, int stride)
{
    const float inv = 1.0f / (2 * r + 1);
    for (int l = 0; l < n; ++l) {
        const float *s = src + l * stride;
        float *o = dst + l * stride;
        float acc = 0;
        for (int i = -r; i <= r; ++i) acc += (i >= 0 && i < n) ? s[i * step] : 0;
        for (int i = 0; i < n; ++i) {
            o[i * step] = acc * inv;
            const int add = i + r + 1, sub = i - r;
            if (add < n) acc += s[add * step];
            if (sub >= 0) acc -= s[sub * step];
        }
    }
}

// Disc of diameter d, Gaussian-blurred with sigma (3 box passes), padded by 3 sigma.
QImage makeBlob(const QColor &c, qreal d, qreal sigma)
{
    const int pad = int(std::ceil(3 * sigma));
    const int n = int(std::ceil(d)) + 2 * pad;
    QVector<float> buf(n * n), tmp(n * n);
    const float r = d / 2, cx = n / 2.0f;
    for (int y = 0; y < n; ++y)
        for (int x = 0; x < n; ++x)
            buf[y * n + x] = qBound(0.0f, r - std::hypot(x + 0.5f - cx, y + 0.5f - cx) + 0.5f, 1.0f);

    const int br = qMax(1, int(std::round((std::sqrt(4.0 * sigma * sigma + 1) - 1) / 2)));
    for (int pass = 0; pass < 3; ++pass) {
        boxBlur(buf.data(), tmp.data(), n, br, 1, n);
        boxBlur(tmp.data(), buf.data(), n, br, n, 1);
    }

    QImage img(n, n, QImage::Format_ARGB32_Premultiplied);
    for (int y = 0; y < n; ++y) {
        QRgb *line = reinterpret_cast<QRgb *>(img.scanLine(y));
        for (int x = 0; x < n; ++x) {
            const float a = qBound(0.0f, buf[y * n + x], 1.0f);
            line[x] = qRgba(int(c.red() * a + 0.5f), int(c.green() * a + 0.5f), int(c.blue() * a + 0.5f), int(255 * a + 0.5f));
        }
    }
    return img;
}

// Mockup geometry as fractions of the CSS .bg box (inset -20%, so 1.4 x the canvas): blob = 55% of
// the box width; left/top place the blob in the box (1 = flush right/bottom), blur(40px) at 420 px
// of the short side, keyframes translate(% of blob) + scale. On 1080x1920 this gives the old
// absolute numbers (blob 1 at -140.4, -249.6 ...).
struct BlobDef { const char *color, *calm; qreal fx, fy, dx, dy, scale, dur; };
const BlobDef kBlobs[3] = {
    {"#1e6fd9", "#1e6fd9", 0.05, 0.05, 0.4, 0.6, 1.2, 18},
    {"#1fa36b", "#1a8f86", 1.00, 0.40, -0.5, -0.4, 0.8, 22},   // right: 0 / top: 40%; teal on the offline page
    {"#8a4fd8", "#8a4fd8", 0.15, 1.00, 0.3, -0.7, 1.3, 26},   // left: 15% / bottom: 0
};

} // namespace

BlobLayer::BlobLayer(QQuickItem *parent) : QQuickPaintedItem(parent)
{
    setOpaquePainting(true);
    connect(this, &QQuickItem::widthChanged, this, &BlobLayer::compose);
    connect(this, &QQuickItem::heightChanged, this, &BlobLayer::compose);
}

void BlobLayer::compose()
{
    if (width() < 1 || height() < 1) return;
    const qreal f = 1.0 / m_down;
    const QSize small(qCeil(width() * f), qCeil(height() * f));
    const qreal bx = -0.2 * width(), by = -0.2 * height(), bw = 1.4 * width(), bh = 1.4 * height();
    const qreal kD = 0.55 * bw;
    if (small != m_builtFor || m_calm != m_builtCalm) {
        const qreal sigma = 40 * qMin(width(), height()) / 420;
        for (int i = 0; i < 3; ++i) m_blobs[i] = makeBlob(QColor(m_calm ? kBlobs[i].calm : kBlobs[i].color), kD * f, sigma * f);
        m_layer = QImage(small, QImage::Format_ARGB32_Premultiplied);
        m_buf = QImage(small, QImage::Format_RGB32);
        m_builtFor = small;
        m_builtCalm = m_calm;
    }

    // Blobs screen-blended with each other (CSS mix-blend-mode: screen inside the .bg group)...
    m_layer.fill(Qt::transparent);
    QPainter lp(&m_layer);
    lp.setRenderHint(QPainter::SmoothPixmapTransform);
    for (int i = 0; i < 3; ++i) {
        const BlobDef &b = kBlobs[i];
        const qreal p = (1 - std::cos(M_PI * m_t / b.dur)) / 2;   // ease-in-out, alternate
        const qreal left = bx + (b.fx == 1.0 ? bw - kD : b.fx * bw), top = by + (b.fy == 1.0 ? bh - kD : b.fy * bh);
        const qreal cx = (left + kD / 2 + b.dx * kD * p) * f;
        const qreal cy = (top + kD / 2 + b.dy * kD * p) * f;
        const qreal s = 1 + (b.scale - 1) * p;
        lp.setCompositionMode(i == 0 ? QPainter::CompositionMode_SourceOver : QPainter::CompositionMode_Screen);
        lp.save();
        lp.translate(cx, cy);
        lp.scale(s, s);
        lp.drawImage(QPointF(-m_blobs[i].width() / 2.0, -m_blobs[i].height() / 2.0), m_blobs[i]);
        lp.restore();
    }
    lp.end();

    // ...then the group at opacity `intensity` over the #07090d frame background.
    m_buf.fill(QColor("#07090d"));
    QPainter bp(&m_buf);
    bp.setOpacity(m_intensity);
    bp.drawImage(0, 0, m_layer);
    bp.end();
    update();
}

void BlobLayer::paint(QPainter *p)
{
    p->setRenderHint(QPainter::SmoothPixmapTransform);
    p->drawImage(QRectF(0, 0, width(), height()), m_buf);
}
