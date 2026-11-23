// Glass panel; children are spread vertically like CSS justify-content: space-between.
// Without an explicit height it is as tall as its content (3 mockup px between children).
import QtQuick

Rectangle {
    id: tile
    default property alias content: col.data
    implicitHeight: col.used + 2 * col.y + Math.max(0, col.children.length - 1) * 3 * S
    color: Qt.rgba(20 / 255, 25 / 255, 34 / 255, 0.6)
    border.color: Qt.rgba(1, 1, 1, 0.06)
    border.width: Math.max(1, Math.round(0.8 * S))
    radius: 7 * S

    Column {
        id: col
        x: 12.85 * S
        y: 9.64 * S
        width: tile.width - 2 * x
        height: tile.height - 2 * y
        readonly property real used: {
            let h = 0
            for (let i = 0; i < children.length; ++i) h += children[i].height
            return h
        }
        spacing: Math.max(0, (height - used) / Math.max(1, children.length - 1))
    }
}
