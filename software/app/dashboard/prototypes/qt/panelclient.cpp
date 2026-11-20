#include "panelclient.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

PanelClient::PanelClient(const QString &host, quint16 port, QObject *parent)
    : QObject(parent), m_host(host), m_port(port)
{
    m_retry.setSingleShot(true);
    connect(&m_retry, &QTimer::timeout, this, &PanelClient::connectNow);
    connect(&m_sock, &QTcpSocket::readyRead, this, &PanelClient::readLines);
    connect(&m_sock, &QTcpSocket::connected, this, [this] {
        m_backoffMs = 500;
        qInfo("panel socket connected to %s:%u", qPrintable(m_host), m_port);
        emit linkChanged();
    });
    connect(&m_sock, &QTcpSocket::disconnected, this, [this] {
        emit linkChanged();
        scheduleReconnect();
    });
    connect(&m_sock, &QTcpSocket::errorOccurred, this, [this](QAbstractSocket::SocketError) {
        if (m_sock.state() != QAbstractSocket::ConnectedState) scheduleReconnect();
    });
    connectNow();
}

void PanelClient::connectNow()
{
    m_sock.abort();
    m_sock.connectToHost(m_host, m_port);
}

void PanelClient::scheduleReconnect()
{
    // Keep showing the last frame; back off 0.5 s -> 5 s.
    if (m_retry.isActive()) return;
    m_retry.start(m_backoffMs);
    m_backoffMs = qMin(m_backoffMs * 2, 5000);
}

void PanelClient::readLines()
{
    while (m_sock.canReadLine()) {
        const QByteArray line = m_sock.readLine().trimmed();
        if (!line.isEmpty()) handle(line);
    }
}

static QString speedText(int mbps)
{
    return mbps >= 1000 && mbps % 1000 == 0 ? QString::number(mbps / 1000) + "G" : QString::number(mbps) + "M";
}

void PanelClient::handle(const QByteArray &line)
{
    QJsonParseError err;
    const QJsonObject o = QJsonDocument::fromJson(line, &err).object();
    if (err.error != QJsonParseError::NoError) return;
    const QString t = o.value("t").toString();

    if (t == "telemetry") {
        for (const QJsonValue &v : o.value("metrics").toArray()) {
            const QJsonObject j = v.toObject();
            Metric &m = m_metrics[j.value("id").toString()];
            m.value = j.value("value").isNull() ? qQNaN() : j.value("value").toDouble();
            m.min = j.value("min").toDouble();
            m.max = j.value("max").toDouble();
            m.warn = j.contains("warn") ? j.value("warn").toDouble() : qQNaN();
            m.unit = j.value("unit").toString();
            m.label = j.value("label").toString();
            m.hist.append(m.value);
            if (m.hist.size() > HistoryLen) m.hist.remove(0, m.hist.size() - HistoryLen);
        }
        if (!m_pcConnected) { m_pcConnected = true; emit linkChanged(); }
        ++m_revision;
        emit updated();
    } else if (t == "status") {
        const QJsonObject eth = o.value("eth").toObject();
        const QString es = eth.value("state").toString();
        const int mbps = eth.value("mbps").toInt();
        if (es == "up") { m_ethState = "ok"; m_ethText = speedText(mbps) + " · " + eth.value("ip").toString(); }
        else if (es == "no_ip") { m_ethState = "warn"; m_ethText = speedText(mbps) + " · No IP"; }
        else { m_ethState = "off"; m_ethText = "ETH down"; }

        const QString ps = o.value("pc").toObject().value("state").toString();
        m_pcState = ps;
        m_pcText = ps == "ok" ? "PC" : ps == "partial" ? "PC partial" : "PC off";

        const QJsonValue dev = o.value("usb_a").toObject().value("device");
        m_usbState = dev.isString() ? "ok" : "off";
        m_usbText = dev.isString() ? dev.toString() : "No USB";
        emit statusChanged();
    } else if (t == "pc") {
        m_pcConnected = o.value("connected").toBool();
        emit linkChanged();
    }
}
