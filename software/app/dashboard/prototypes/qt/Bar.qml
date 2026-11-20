// Thin progress bar under a row, range = metric min..max.
import QtQuick

Rectangle {
    id: bar
    property string metric
    property color tint
    width: parent.width
    height: 4 * S
    radius: 2 * S
    color: Qt.rgba(1, 1, 1, 0.08)

    readonly property real frac: {
        panel.revision
        const v = panel.value(metric), lo = panel.min(metric), hi = panel.max(metric)
        return isNaN(v) || hi === lo ? 0 : Math.max(0, Math.min(1, (v - lo) / (hi - lo)))
    }

    Rectangle {
        height: parent.height
        radius: parent.radius
        color: bar.tint
        width: parent.width * bar.frac
        Behavior on width { NumberAnimation { duration: 600; easing.type: Easing.InOutQuad } }
    }
}
