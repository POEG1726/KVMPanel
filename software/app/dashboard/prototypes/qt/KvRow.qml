// "label ........ value" text row of the offline page; `dim` greys the value (No signal, Idle, None).
// A hidden row takes no height, so Tile does not leave a gap for it.
import QtQuick

Item {
    property alias label: l.text
    property alias value: v.text
    property bool dim: false

    width: parent ? parent.width : 0
    height: visible ? Math.max(l.implicitHeight, v.implicitHeight) : 0

    T {
        id: l
        anchors.verticalCenter: parent.verticalCenter
        color: "#c5cfdc"
    }
    T {
        id: v
        anchors.right: parent.right
        anchors.left: l.right
        anchors.leftMargin: 8 * S
        anchors.verticalCenter: parent.verticalCenter
        horizontalAlignment: Text.AlignRight
        elide: Text.ElideLeft
        font.weight: Font.DemiBold
        color: parent.dim ? "#6b7585" : "#e2e8f0"
    }
}
