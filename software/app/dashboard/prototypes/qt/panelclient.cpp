#include "panelclient.h"

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

PanelClient::PanelClient(const QString &host, quint16 port, QObject *parent)
    : QObject(parent), m_host(host), m_port(port)
{
    m_retry.setSingleShot(true);
    connect(&m_retry, &QTimer::timeout, this, &PanelClient::connectNow);
    m_telemetryWatch.setSingleShot(true);
    m_telemetryWatch.setInterval(3000);
    connect(&m_telemetryWatch, &QTimer::timeout, this, [this] { setPcConnected(false); });
    m_powerOnClear.setSingleShot(true);
    connect(&m_powerOnClear, &QTimer::timeout, this, [this] { setPowerOn(QString()); });
    connect(&m_sock, &QTcpSocket::readyRead, this, &PanelClient::readLines);
    connect(&m_sock, &QTcpSocket::connected, this, [this] {
        m_backoffMs = 500;
        qInfo("panel socket connected to %s:%u", qPrintable(m_host), m_port);
        emit linkChanged();
    });
    connect(&m_sock, &QTcpSocket::disconnected, this, [this] {
        m_powerOnId = -1;
        if (m_powerOn == "pending") setPowerOn(QString());
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

void PanelClient::setPcConnected(bool c)
{
    if (c == m_pcConnected) return;
    m_pcConnected = c;
    emit linkChanged();
}

void PanelClient::setPowerOn(const QString &s)
{
    if (s == m_powerOn) return;
    m_powerOn = s;
    emit powerOnChanged();
}

void PanelClient::powerOnPc()
{
    if (!linked() || m_powerOn == "pending") return;
    m_powerOnId = m_nextId++;
    const QJsonObject req{{"t", "action"}, {"id", m_powerOnId}, {"action", "pc_power_on"}};
    m_sock.write(QJsonDocument(req).toJson(QJsonDocument::Compact) + '\n');
    m_powerOnClear.stop();
    setPowerOn("pending");
}

static QString speedText(int mbps)
{
    return mbps >= 1000 && mbps % 1000 == 0 ? QString::number(mbps / 1000) + "G" : QString::number(mbps) + "M";
}

void PanelClient::push(const QString &id, double value, double min, double max, const QString &unit, const QString &label)
{
    Metric &m = m_metrics[id];
    m.value = value;
    m.min = min;
    m.max = max;
    m.unit = unit;
    m.label = label;
    m.hist.append(value);
    if (m.hist.size() > HistoryLen) m.hist.remove(0, m.hist.size() - HistoryLen);
}

void PanelClient::handleBoard(const QJsonObject &o)
{
    const auto num = [](const QJsonValue &v) { return v.isDouble() ? v.toDouble() : qQNaN(); };
    const QJsonObject eth = o.value("eth").toObject();
    const QJsonObject mem = o.value("mem").toObject();
    // Fixed sparkline ranges (mockup): RX 0-2 MB/s, TX 0-10 MB/s, SoC 30-90 °C.
    push("board.rx", num(eth.value("rx_bps")) / 8e6, 0, 2, "MB/s", "↓ RX");
    push("board.tx", num(eth.value("tx_bps")) / 8e6, 0, 10, "MB/s", "↑ TX");
    push("board.soc", num(o.value("soc_temp_c")), 30, 90, "°C", "SoC temp");
    push("board.cpu", num(o.value("cpu_pct")), 0, 100, "%", "CPU");
    push("board.mem", num(mem.value("used")) / 1e6, 0, mem.value("total").toDouble() / 1e6, "MB", "Memory");
    m_board = o.toVariantMap();
    if (m_powerOn == "pending" && o.value("pc_power").toString() == "on") {
        m_powerOnId = -1;
        setPowerOn(QString());
    }
    ++m_revision;
    emit boardChanged();
    emit updated();
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
        m_lastSeenTs = o.value("ts").toDouble();
        m_telemetryWatch.start();
        setPcConnected(true);
        ++m_revision;
        emit updated();
    } else if (t == "board") {
        handleBoard(o);
    } else if (t == "status") {
        const QJsonObject eth = o.value("eth").toObject();
        const QString es = eth.value("state").toString();
        const int mbps = eth.value("mbps").toInt();
        m_ip = eth.value("ip").toString();
        if (es == "up") { m_ethState = "ok"; m_ethText = speedText(mbps) + " · " + m_ip; }
        else if (es == "no_ip") { m_ethState = "warn"; m_ethText = speedText(mbps) + " · No IP"; }
        else { m_ethState = "off"; m_ethText = "ETH down"; }

        const QString ps = o.value("pc").toObject().value("state").toString();
        m_pcLink = ps;
        m_pcState = ps == "partial" ? "warn" : ps;
        m_pcText = ps == "ok" ? "PC" : ps == "partial" ? "PC partial" : "PC off";

        const QJsonValue dev = o.value("usb_a").toObject().value("device");
        m_usbState = dev.isString() ? "ok" : "off";
        m_usbText = dev.isString() ? dev.toString() : "No USB";
        emit statusChanged();
    } else if (t == "pc") {
        const QJsonValue seen = o.value("last_seen_ts");
        if (seen.isDouble()) m_lastSeenTs = seen.toDouble();
        const bool c = o.value("connected").toBool();
        if (c) m_telemetryWatch.start();
        else m_telemetryWatch.stop();
        if (c == m_pcConnected) emit linkChanged();   // last_seen_ts may still have changed
        setPcConnected(c);
    } else if (t == "config") {
        const int v = o.value("config").toObject().value("panel").toObject().value("offline_sleep_min").toInt(5);
        if (v != m_offlineSleepMin) { m_offlineSleepMin = v; emit configChanged(); }
    } else if (t == "result" && m_powerOnId >= 0 && o.value("id").toInt(-1) == m_powerOnId) {
        if (!o.value("ok").toBool()) {
            qWarning("pc_power_on refused: %s", qPrintable(o.value("error").toString()));
            m_powerOnId = -1;
            setPowerOn("failed");
            m_powerOnClear.start(3000);
        }
        // ok: stay pending until `board` reports the power LED on.
    }
}
