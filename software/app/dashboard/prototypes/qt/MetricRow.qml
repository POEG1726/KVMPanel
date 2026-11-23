// label | framed sparkline (84 px) | value (96 px); value flashes red above `warn`.
import QtQuick
import Proto 1.0

Item {
    id: row
    property string metric
    property string label: { panel.revision; return panel.label(metric) }
    property color tint
    property int decimals: 0
    property bool ofMax: false      // "24.0 / 32 GB"
    property var fmt: null          // optional value -> text
    property real labelWidth: 0     // > 0: fixed label column, the sparkline stretches (landscape)

    width: parent.width
    height: 18 * S

    readonly property real v: { panel.revision; return panel.value(metric) }
    readonly property real warnAt: { panel.revision; return panel.warn(metric) }
    readonly property bool hot: !isNaN(v) && !isNaN(warnAt) && v > warnAt

    T {
        anchors.left: parent.left
        anchors.right: spark.left
        anchors.rightMargin: 6 * S
        anchors.verticalCenter: parent.verticalCenter
        text: row.label
        elide: Text.ElideRight
    }

    Sparkline {
        id: spark
        x: parent.width - 96 * S - 6 * S - width
        width: row.labelWidth > 0 ? Math.max(20 * S, parent.width - row.labelWidth - 96 * S - 12 * S) : 84 * S
        height: 18 * S
        source: panel
        metrics: [row.metric]
        colors: [row.tint]
        unit: S
    }

    T {
        id: val
        anchors.right: parent.right
        anchors.verticalCenter: parent.verticalCenter
        width: 96 * S
        horizontalAlignment: Text.AlignRight
        font.weight: Font.DemiBold
        text: {
            if (isNaN(row.v)) return "--"
            if (row.fmt) return row.fmt(row.v)
            const u = panel.unit(row.metric)
            const s = row.v.toFixed(row.decimals)
            if (row.ofMax) return s + " / " + Math.round(panel.max(row.metric)) + " " + u
            return u ? s + " " + u : s
        }
        SequentialAnimation on color {
            running: row.hot
            loops: Animation.Infinite
            ColorAnimation { to: "#ff6b6b"; duration: 600 }
            ColorAnimation { to: "#e2e8f0"; duration: 600 }
        }
    }
    onHotChanged: if (!hot) val.color = "#e2e8f0"
}
