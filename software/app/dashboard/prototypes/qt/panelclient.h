// Panel socket client: reads kvmd NDJSON over TCP and keeps the latest metrics plus 60 s of history.
#pragma once

#include <QHash>
#include <QObject>
#include <QTcpSocket>
#include <QTimer>
#include <QVector>

class PanelClient : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int revision READ revision NOTIFY updated)
    Q_PROPERTY(bool linked READ linked NOTIFY linkChanged)
    Q_PROPERTY(bool pcConnected READ pcConnected NOTIFY linkChanged)
    Q_PROPERTY(QString ethState MEMBER m_ethState NOTIFY statusChanged)
    Q_PROPERTY(QString ethText MEMBER m_ethText NOTIFY statusChanged)
    Q_PROPERTY(QString pcState MEMBER m_pcState NOTIFY statusChanged)
    Q_PROPERTY(QString pcText MEMBER m_pcText NOTIFY statusChanged)
    Q_PROPERTY(QString usbState MEMBER m_usbState NOTIFY statusChanged)
    Q_PROPERTY(QString usbText MEMBER m_usbText NOTIFY statusChanged)

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

    Q_INVOKABLE double value(const QString &id) const { auto m = find(id); return m ? m->value : qQNaN(); }
    Q_INVOKABLE double min(const QString &id) const { auto m = find(id); return m ? m->min : 0; }
    Q_INVOKABLE double max(const QString &id) const { auto m = find(id); return m ? m->max : 1; }
    Q_INVOKABLE double warn(const QString &id) const { auto m = find(id); return m ? m->warn : qQNaN(); }
    Q_INVOKABLE QString unit(const QString &id) const { auto m = find(id); return m ? m->unit : QString(); }
    Q_INVOKABLE QString label(const QString &id) const { auto m = find(id); return m ? m->label : QString(); }

    const Metric *find(const QString &id) const { auto it = m_metrics.constFind(id); return it == m_metrics.cend() ? nullptr : &*it; }

signals:
    void updated();
    void linkChanged();
    void statusChanged();

private:
    void connectNow();
    void scheduleReconnect();
    void readLines();
    void handle(const QByteArray &line);

    QString m_host;
    quint16 m_port;
    QTcpSocket m_sock;
    QTimer m_retry;
    int m_backoffMs = 500;
    QHash<QString, Metric> m_metrics;
    int m_revision = 0;
    bool m_pcConnected = true;
    QString m_ethState = "off", m_ethText = "ETH --";
    QString m_pcState = "off", m_pcText = "PC";
    QString m_usbState = "off", m_usbText = "No USB";
};
