// E2 background: blurred colour blobs (BlobLayer, C++) drifting, plus a faint grid scrolling down
// with a top/bottom fade. Driven by a Timer at `animFps` instead of vsync animations.
import QtQuick
import Proto 1.0

Item {
    id: bd
    property real intensity: 0.55     // 0.3 + 0.5 * max(cpu.load, gpu.load) / 100
    property bool calm: false         // dim teal palette of the PC-offline page
    property bool running: true       // false while the screen is off
    property real t: 0                // seconds since start
    readonly property double t0: Date.now()

    // Intensity eases toward its target on the same tick (no extra 60 Hz animation).
    property real shown: intensity

    Timer {
        interval: Math.max(1, Math.round(1000 / animFps))
        running: bd.running
        repeat: true
        onTriggered: {
            bd.shown += (bd.intensity - bd.shown) * 0.1
            bd.t = (Date.now() - bd.t0) / 1000
        }
    }

    BlobLayer {
        anchors.fill: parent
        intensity: bd.shown
        calm: bd.calm
        t: bd.t
    }

    // Grid: 24 px cells, rgba(94,196,255,.25) lines, layer opacity .25, mask transparent/30%/70%/transparent.
    Item {
        id: grid
        visible: false // grid removed from the design
        anchors.fill: parent
        opacity: 0.25
        readonly property real cell: 24 * S
        readonly property real lw: Math.max(1, Math.round(0.8 * S))
        readonly property color lc: Qt.rgba(94 / 255, 196 / 255, 1, 0.25)
        readonly property real phase: (bd.t % 8) / 8

        function mask(f) { return f < 0 || f > 1 ? 0 : f < 0.3 ? f / 0.3 : f > 0.7 ? (1 - f) / 0.3 : 1 }

        Repeater {
            model: Math.ceil(bd.width / grid.cell) + 1
            Rectangle {
                required property int index
                x: index * grid.cell
                width: grid.lw
                height: bd.height
                gradient: Gradient {
                    GradientStop { position: 0.0; color: "transparent" }
                    GradientStop { position: 0.3; color: grid.lc }
                    GradientStop { position: 0.7; color: grid.lc }
                    GradientStop { position: 1.0; color: "transparent" }
                }
            }
        }
        Repeater {
            model: Math.ceil(bd.height / grid.cell) + 2
            Rectangle {
                required property int index
                y: (index - 1 + grid.phase) * grid.cell
                width: bd.width
                height: grid.lw
                color: grid.lc
                opacity: grid.mask(y / bd.height)
            }
        }
    }
}
