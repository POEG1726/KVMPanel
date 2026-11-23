// Window: portrait 1080x1920 or landscape 1920x1080 canvas (--landscape), scaled into the window by
// viewScale. Shows the main dashboard while the PC host program sends telemetry, otherwise the
// PC-offline page. The offline page counts down config.panel.offline_sleep_min and then blanks the
// screen until a click or key press.
import QtQuick
import QtQuick.Window

Window {
    id: win
    readonly property int cw: landscape ? 1920 : 1080
    readonly property int ch: landscape ? 1080 : 1920
    width: Math.round(cw * viewScale)
    height: Math.round(ch * viewScale)
    visible: true
    color: "#07090d"
    title: "KVMPanel dashboard prototype (Qt Quick)"

    function nz(v) { return isNaN(v) ? 0 : v }

    readonly property bool offline: !panel.pcConnected
    property int countdown: -1
    property bool screenOff: false

    function wake() {
        countdown = panel.offlineSleepMin > 0 ? panel.offlineSleepMin * 60 : -1
        screenOff = false
    }
    onOfflineChanged: wake()
    Component.onCompleted: wake()
    Connections { target: panel; function onConfigChanged() { if (win.offline) win.wake() } }
    Timer {
        interval: 1000; repeat: true
        running: win.offline && !win.screenOff && win.countdown > 0
        onTriggered: if (--win.countdown === 0) win.screenOff = true
    }

    Item {
        id: canvas
        width: win.cw
        height: win.ch
        scale: viewScale
        transformOrigin: Item.TopLeft

        Backdrop {
            anchors.fill: parent
            running: !win.screenOff
            calm: win.offline
            intensity: {
                panel.revision
                if (win.offline) return 0.18 + 0.25 * win.nz(panel.value("board.cpu")) / 100
                return 0.3 + 0.5 * Math.max(win.nz(panel.value("cpu.load")), win.nz(panel.value("gpu.load"))) / 100
            }
        }

        // The landscape main screen (layout B) is not built yet: show the portrait one letterboxed.
        MainScreen {
            visible: !win.offline
            scale: landscape ? win.ch / 1920 : 1
            transformOrigin: Item.TopLeft
            x: landscape ? (win.cw - width * scale) / 2 : 0
        }

        OfflinePage {
            visible: win.offline
            anchors.fill: parent
            landscape: win.cw > win.ch
            countdown: win.countdown
            onPowerOnRequested: panel.powerOnPc()
        }

        // kvmd socket down: keep the last frame, show a hint.
        Rectangle {
            visible: !panel.linked
            anchors.centerIn: parent
            width: hint.implicitWidth + 40 * S
            height: hint.implicitHeight + 16 * S
            radius: 8 * S
            color: Qt.rgba(0, 0, 0, 0.7)
            T {
                id: hint
                anchors.centerIn: parent
                text: "Connecting to kvmd…"
                color: "#ffb454"
                font.pixelSize: 14 * S
            }
        }

        // Screen off after the offline countdown; any click or key wakes it.
        Rectangle {
            visible: win.screenOff
            anchors.fill: parent
            color: "black"
            focus: win.screenOff
            Keys.onPressed: (e) => { win.wake(); e.accepted = true }
            MouseArea { anchors.fill: parent; onClicked: win.wake() }
        }
    }
}
