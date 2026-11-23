// Panel socket client: reads kvmd NDJSON over TCP and keeps the latest metrics plus 60 s of history.
// The board's own numbers from `board` are kept as metrics too (board.rx, board.tx, board.soc,
// board.cpu, board.mem) so the PC-offline page can reuse Sparkline.
#pragma once

#include <QHash>
#include <QObject>
#include <QTcpSocket>
#include <QTimer>
#include <QVariantMap>
#include <QVector>

class PanelClient : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int revision READ revision NOTIFY updated)
    Q_PROPERTY(bool linked READ linked NOTIFY linkChanged)
    Q_PROPERTY(bool pcConnected READ pcConnected NOTIFY linkChanged)
    Q_PROPERTY(double lastSeenTs READ lastSeenTs NOTIFY linkChanged)      // Unix ms, NaN = never
    Q_PROPERTY(QString ethState MEMBER m_ethState NOTIFY statusChanged)
    Q_PROPERTY(QString ethText MEMBER m_ethText NOTIFY statusChanged)
    Q_PROPERTY(QString pcState MEMBER m_pcState NOTIFY statusChanged)
    Q_PROPERTY(QString pcText MEMBER m_pcText NOTIFY statusChanged)
    Q_PROPERTY(QString usbState MEMBER m_usbState NOTIFY statusChanged)
    Q_PROPERTY(QString usbText MEMBER m_usbText NOTIFY statusChanged)
    Q_PROPERTY(QString pcLink MEMBER m_pcLink NOTIFY statusChanged)       // status.pc.state: ok / partial / off
    Q_PROPERTY(QString ip MEMBER m_ip NOTIFY statusChanged)
    Q_PROPERTY(QVariantMap board READ board NOTIFY boardChanged)          // last `board` message, empty until one arrives
    Q_PROPERTY(int offlineSleepMin READ offlineSleepMin NOTIFY configChanged)
    Q_PROPERTY(QString powerOn READ powerOn NOTIFY powerOnChanged)        // "", "pending", "failed"

public:
    static constexpr int HistoryLen = 60;

    struct Metric {
        double value = qQNaN();     // NaN = null (not reported)
        double min = 0, max = 1;
        double warn = qQNaN();      // NaN = no threshold
        QString unit, label;
        QVector<double> hist;       // oldest first, at most HistoryLen
    };

    PanelClient(const QString &host, quint16 port, QObject *parent = nullptr);

    int revision() const { return m_revision; }
    bool linked() const { return m_sock.state() == QAbstractSocket::ConnectedState; }
    bool pcConnected() const { return m_pcConnected; }
    double lastSeenTs() const { return m_lastSeenTs; }
    QVariantMap board() const { return m_board; }
    int offlineSleepMin() const { return m_offlineSleepMin; }
    QString powerOn() const { return m_powerOn; }

    Q_INVOKABLE double value(const QString &id) const { auto m = find(id); return m ? m->value : qQNaN(); }
    Q_INVOKABLE double min(const QString &id) const { auto m = find(id); return m ? m->min : 0; }
    Q_INVOKABLE double max(const QString &id) const { auto m = find(id); return m ? m->max : 1; }
    Q_INVOKABLE double warn(const QString &id) const { auto m = find(id); return m ? m->warn : qQNaN(); }
    Q_INVOKABLE QString unit(const QString &id) const { auto m = find(id); return m ? m->unit : QString(); }
    Q_INVOKABLE QString label(const QString &id) const { auto m = find(id); return m ? m->label : QString(); }
    // Short-press the PC power button; kvmd refuses it while the PC is on.
    Q_INVOKABLE void powerOnPc();

    const Metric *find(const QString &id) const { auto it = m_metrics.constFind(id); return it == m_metrics.cend() ? nullptr : &*it; }

signals:
    void updated();
    void linkChanged();
    void statusChanged();
    void boardChanged();
    void configChanged();
    void powerOnChanged();

private:
    void connectNow();
    void scheduleReconnect();
    void readLines();
    void handle(const QByteArray &line);
    void handleBoard(const QJsonObject &o);
    void push(const QString &id, double value, double min, double max, const QString &unit, const QString &label);
    void setPcConnected(bool c);
    void setPowerOn(const QString &s);

    QString m_host;
    quint16 m_port;
    QTcpSocket m_sock;
    QTimer m_retry;
    QTimer m_telemetryWatch;    // no telemetry for 3 s = host program gone, even without a `pc` message
    QTimer m_powerOnClear;
    int m_backoffMs = 500;
    int m_nextId = 1;
    int m_powerOnId = -1;
    QHash<QString, Metric> m_metrics;
    int m_revision = 0;
    bool m_pcConnected = false;
    double m_lastSeenTs = qQNaN();
    QVariantMap m_board;
    int m_offlineSleepMin = 5;
    QString m_powerOn;
    QString m_ethState = "off", m_ethText = "ETH --";
    QString m_pcState = "off", m_pcText = "PC";
    QString m_usbState = "off", m_usbText = "No USB";
    QString m_pcLink = "off";
    QString m_ip;
};
