// Qt Quick dashboard prototype: software scene graph, 1080x1920 portrait canvas, data from the panel socket.
#include "bloblayer.h"
#include "panelclient.h"
#include "sparkline.h"

#include <QCommandLineParser>
#include <QElapsedTimer>
#include <QFont>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQmlContext>
#include <QQuickWindow>
#include <QSGRendererInterface>
#include <QTimer>

int main(int argc, char *argv[])
{
    // CPU-only path, like the board without a usable GPU.
    if (qEnvironmentVariableIsEmpty("QT_QUICK_BACKEND")) qputenv("QT_QUICK_BACKEND", "software");
    QQuickWindow::setGraphicsApi(QSGRendererInterface::Software);

    QGuiApplication app(argc, argv);
    QGuiApplication::setFont(QFont("Ubuntu Sans"));   // Segoe UI in the mockup; not available on Linux
    QCommandLineParser cli;
    cli.addHelpOption();
    cli.addOptions({
        {"host", "panel socket host", "host", qEnvironmentVariable("KVMPANEL_PANEL_HOST", "127.0.0.1")},
        {"port", "panel socket port", "port", qEnvironmentVariable("KVMPANEL_PANEL_PORT", "7701")},
        {"scale", "window scale of the 1080x1920 canvas", "f", "0.5"},
        {"fps", "background animation tick rate", "n", "30"},
        {"screenshot", "grab the window to this PNG after --delay ms and quit", "png"},
        {"delay", "screenshot delay (ms)", "ms", "5000"},
        {"duration", "quit after this many seconds (0 = run forever)", "s", "0"},
    });
    cli.process(app);

    const bool shot = cli.isSet("screenshot");
    const double scale = shot ? 1.0 : cli.value("scale").toDouble();

    PanelClient panel(cli.value("host"), cli.value("port").toUShort());
    qmlRegisterType<Sparkline>("Proto", 1, 0, "Sparkline");
    qmlRegisterType<BlobLayer>("Proto", 1, 0, "BlobLayer");

    QQmlApplicationEngine engine;
    engine.rootContext()->setContextProperty("panel", &panel);
    engine.rootContext()->setContextProperty("viewScale", scale);
    engine.rootContext()->setContextProperty("S", 1080.0 / 420.0);   // mockup px -> screen px
    engine.rootContext()->setContextProperty("animFps", cli.value("fps").toInt());
    engine.load(QUrl("qrc:/Dashboard.qml"));
    if (engine.rootObjects().isEmpty()) return 1;
    auto *win = qobject_cast<QQuickWindow *>(engine.rootObjects().first());

    // Frame counter: prints rendered frames per second every 5 s.
    int frames = 0;
    QElapsedTimer clock;
    clock.start();
    QObject::connect(win, &QQuickWindow::frameSwapped, [&] { ++frames; });
    QTimer fpsTimer;
    QObject::connect(&fpsTimer, &QTimer::timeout, [&] {
        qInfo("fps %.1f", frames * 1000.0 / clock.restart());
        frames = 0;
    });
    fpsTimer.start(5000);

    if (shot) {
        QTimer::singleShot(cli.value("delay").toInt(), [&] {
            const QImage img = win->grabWindow();
            const bool ok = img.save(cli.value("screenshot"));
            qInfo("screenshot %dx%d -> %s (%s)", img.width(), img.height(),
                  qPrintable(cli.value("screenshot")), ok ? "ok" : "FAILED");
            app.exit(ok ? 0 : 2);
        });
    }
    if (const int s = cli.value("duration").toInt(); s > 0) QTimer::singleShot(s * 1000, &app, &QGuiApplication::quit);

    return app.exec();
}
