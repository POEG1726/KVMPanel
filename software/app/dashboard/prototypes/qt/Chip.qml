// Status chip: glowing dot (ok green / warn orange / off grey), 12x12 SVG-path icon, text.
import QtQuick
import QtQuick.Shapes

Rectangle {
    id: chip
    property string level: "ok"
    property string label
    property string icon
    readonly property color dotColor: level === "ok" ? "#3fb950" : level === "warn" ? "#ffb454" : "#5a6270"
    readonly property color fg: level === "off" ? "#6b7585" : "#c5cfdc"

    implicitWidth: row.implicitWidth + 12 * S
    implicitHeight: row.implicitHeight + 2 * S
    radius: 9 * S
    color: Qt.rgba(1, 1, 1, 0.07)

    Row {
        id: row
        anchors.centerIn: parent
        spacing: 3 * S

        Item {
            width: 6 * S
            height: 6 * S
            anchors.verticalCenter: parent.verticalCenter
            Rectangle {   // box-shadow 0 0 5px approximation
                visible: chip.level !== "off"
                anchors.centerIn: parent
                width: 12 * S; height: width; radius: width / 2
                color: chip.dotColor
                opacity: 0.25
            }
            Rectangle { anchors.fill: parent; radius: width / 2; color: chip.dotColor }
        }

        Shape {
            width: 11 * S
            height: 11 * S
            anchors.verticalCenter: parent.verticalCenter
            ShapePath {
                scale: Qt.size(11 * S / 12, 11 * S / 12)
                strokeColor: chip.fg
                strokeWidth: 1.6 * 11 / 12 * S
                fillColor: "transparent"
                capStyle: ShapePath.RoundCap
                joinStyle: ShapePath.RoundJoin
                PathSvg { path: chip.icon }
            }
        }

        T {
            text: chip.label
            color: chip.fg
            font.pixelSize: 9 * S
        }
    }
}
