// Main dashboard screen (mockup e2-v5.html): 1080x1920 canvas, scaled into the window by viewScale.
// S = 1080 / 420 maps mockup px to screen px.
import QtQuick
import QtQuick.Window
import Proto 1.0

Window {
    id: win
    width: Math.round(1080 * viewScale)
    height: Math.round(1920 * viewScale)
    visible: true
    color: "#07090d"
    title: "KVMPanel dashboard prototype (Qt Quick)"

    readonly property color cCpu: "#5ec4ff"
    readonly property color cGpu: "#7ee787"
    readonly property color cMem: "#d2a8ff"
    readonly property color cPwr: "#ffb454"

    function nz(v) { return isNaN(v) ? 0 : v }

    Item {
        id: canvas
        width: 1080
        height: 1920
        scale: viewScale
        transformOrigin: Item.TopLeft

        Backdrop {
            anchors.fill: parent
            intensity: {
                panel.revision
                return panel.revision > 0
                    ? 0.3 + 0.5 * Math.max(nz(panel.value("cpu.load")), nz(panel.value("gpu.load"))) / 100
                    : 0.55
            }
        }

        Column {
            id: scr
            x: 9.24 * S
            y: 9.24 * S
            width: parent.width - 2 * x
            height: parent.height - 2 * y
            spacing: 0.012 * height
            readonly property real unit: (height - 3 * spacing) / 30.5   // flex 10 / 8 / 7 / 5.5

            // ---------------- CPU ----------------
            Tile {
                width: scr.width
                height: 10 * scr.unit

                Item {
                    width: parent.width
                    height: Math.max(cpuTitle.implicitHeight, statusBar.implicitHeight) + 2 * S
                    T {
                        id: cpuTitle
                        anchors.verticalCenter: parent.verticalCenter
                        text: "CPU · Ryzen 9 9950X3D"
                        color: win.cCpu
                        font.pixelSize: 10 * S
                        font.letterSpacing: 0.6 * S
                        font.capitalization: Font.AllUppercase
                    }
                    Row {
                        id: statusBar
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        spacing: 5 * S
                        Chip {
                            level: panel.ethState; label: panel.ethText
                            icon: "M3 3H9Q10 3 10 4V8Q10 9 9 9H3Q2 9 2 8V4Q2 3 3 3ZM4 9v1.5M6 9v1.5M8 9v1.5"
                        }
                        Chip {
                            level: panel.pcState; label: panel.pcText
                            icon: "M2.5 2H9.5Q10.5 2 10.5 3V7Q10.5 8 9.5 8H2.5Q1.5 8 1.5 7V3Q1.5 2 2.5 2ZM4 10.5h4M6 8v2.5"
                        }
                        Chip {
                            level: panel.usbState; label: panel.usbText
                            icon: "M6 1v8M4 3l2-2 2 2M3 6v1.5l3 1.5M9 5v1.5L6 8"
                        }
                    }
                }

                Item {   // 16 core-load bars, CCD0 brighter
                    id: cores
                    width: parent.width
                    height: 34 * S
                    readonly property real gap: 3 * S
                    Repeater {
                        model: 16
                        Rectangle {
                            required property int index
                            readonly property real load: { panel.revision; return nz(panel.value("cpu.core." + index)) }
                            width: (cores.width - 15 * cores.gap) / 16
                            x: index * (width + cores.gap)
                            height: cores.height * Math.max(3, Math.min(100, load)) / 100
                            y: cores.height - height
                            radius: 2 * S
                            color: index < 8 ? "#5ec4ff" : "#3a8fc0"
                            Behavior on height { NumberAnimation { duration: 600; easing.type: Easing.InOutQuad } }
                        }
                    }
                }

                Item {
                    width: parent.width
                    height: ccd0.implicitHeight
                    T { id: ccd0; text: "CCD0 · V-Cache"; color: "#8b97aa"; font.pixelSize: 9 * S }
                    T { anchors.right: parent.right; text: "CCD1"; color: "#8b97aa"; font.pixelSize: 9 * S }
                }

                MetricRow { metric: "cpu.clock_eff"; tint: win.cCpu; decimals: 2 }
                MetricRow { metric: "cpu.bclk"; tint: win.cCpu; decimals: 2 }
                MetricRow { metric: "cpu.fclk"; tint: win.cCpu }
                MetricRow { metric: "cpu.vcore"; tint: win.cCpu; decimals: 3 }
                MetricRow { metric: "cpu.tctl"; tint: win.cCpu; decimals: 1 }
                MetricRow { metric: "cpu.load"; tint: win.cCpu }
                Bar { metric: "cpu.load"; tint: win.cCpu }
                MetricRow { metric: "cpu.ppt"; tint: win.cCpu }
            }

            // ---------------- GPU ----------------
            Tile {
                width: scr.width
                height: 8 * scr.unit
                T {
                    text: "GPU · RTX 5090"; color: win.cGpu
                    font.pixelSize: 10 * S; font.letterSpacing: 0.6 * S; font.capitalization: Font.AllUppercase
                }
                MetricRow { metric: "gpu.clock"; tint: win.cGpu }
                MetricRow { metric: "gpu.temp"; tint: win.cGpu }
                MetricRow { metric: "gpu.mem_junction"; tint: win.cGpu }
                MetricRow { metric: "gpu.load"; tint: win.cGpu }
                Bar { metric: "gpu.load"; tint: win.cGpu }
                MetricRow { metric: "gpu.vram_used"; tint: win.cGpu; decimals: 1; ofMax: true }
                Bar { metric: "gpu.vram_used"; tint: win.cGpu }
                MetricRow { metric: "gpu.power"; tint: win.cGpu }
            }

            // ---------------- Memory ----------------
            Tile {
                id: memTile
                width: scr.width
                height: 7 * scr.unit
                readonly property var slots: ["a1", "a2", "b1", "b2"]
                readonly property var palette: ["#d2a8ff", "#ff9bd2", "#8bc8ff", "#ffb454"]
                // populated slots, in order, for the chart and its legend
                readonly property var shown: {
                    panel.revision
                    return slots.filter(s => !isNaN(panel.value("mem.dimm." + s)))
                }

                T {
                    text: "Memory · DDR5-6000 2×32G"; color: win.cMem
                    font.pixelSize: 10 * S; font.letterSpacing: 0.6 * S; font.capitalization: Font.AllUppercase
                }
                MetricRow { metric: "mem.used"; tint: win.cMem; decimals: 1; ofMax: true }
                Bar { metric: "mem.used"; tint: win.cMem }

                Item {
                    width: parent.width
                    height: dimmLabel.implicitHeight
                    T { id: dimmLabel; text: "DIMM A1 / A2 / B1 / B2" }
                    Row {
                        anchors.right: parent.right
                        Repeater {
                            model: memTile.slots
                            Row {
                                required property string modelData
                                required property int index
                                readonly property real v: { panel.revision; return panel.value("mem.dimm." + modelData) }
                                readonly property real w: { panel.revision; return panel.warn("mem.dimm." + modelData) }
                                T {
                                    text: isNaN(parent.v) ? "--" : parent.v.toFixed(1)
                                    color: isNaN(parent.v) ? "#5a6270" : "#e2e8f0"
                                    font.weight: Font.DemiBold
                                    SequentialAnimation on color {
                                        running: !isNaN(parent.v) && !isNaN(parent.w) && parent.v > parent.w
                                        loops: Animation.Infinite
                                        ColorAnimation { to: "#ff6b6b"; duration: 600 }
                                        ColorAnimation { to: "#e2e8f0"; duration: 600 }
                                    }
                                }
                                T { text: index < 3 ? " / " : " °C"; font.weight: Font.DemiBold }
                            }
                        }
                    }
                }

                Item {
                    width: parent.width
                    height: 62 * S
                    Sparkline {
                        y: 2 * S
                        width: parent.width
                        height: 58 * S
                        big: true
                        unit: S
                        source: panel
                        metrics: memTile.shown.map(s => "mem.dimm." + s)
                        colors: memTile.shown.map((s, i) => memTile.palette[i])
                    }
                }

                Row {
                    anchors.right: parent.right
                    spacing: 10 * S
                    Repeater {
                        model: memTile.shown
                        T {
                            required property string modelData
                            required property int index
                            text: "— " + modelData.toUpperCase()
                            color: memTile.palette[index]
                            font.pixelSize: 9 * S
                        }
                    }
                    T {
                        text: { panel.revision; return Math.round(panel.min("mem.dimm.a2")) + "–" + Math.round(panel.max("mem.dimm.a2")) + " °C · last 60 s" }
                        color: "#8b97aa"
                        font.pixelSize: 9 * S
                    }
                }

                Item {
                    width: parent.width
                    height: clkLabel.implicitHeight
                    T { id: clkLabel; text: "MCLK / UCLK" }
                    T {
                        anchors.right: parent.right
                        font.weight: Font.DemiBold
                        text: {
                            panel.revision
                            const f = id => { const v = panel.value(id); return isNaN(v) ? "--" : Math.round(v) }
                            return f("mem.mclk") + " / " + f("mem.uclk")
                        }
                    }
                }
            }

            // ---------------- System ----------------
            Tile {
                width: scr.width
                height: 5.5 * scr.unit
                T {
                    text: "System · B850M AORUS ELITE ICE"; color: win.cPwr
                    font.pixelSize: 10 * S; font.letterSpacing: 0.6 * S; font.capitalization: Font.AllUppercase
                }
                MetricRow { metric: "sys.power"; tint: win.cPwr }
                MetricRow { metric: "ssd.0.temp"; tint: win.cPwr }
                MetricRow { metric: "ssd.1.temp"; tint: win.cPwr }
                MetricRow { metric: "net.rx"; tint: win.cPwr; decimals: 1 }
                MetricRow { metric: "net.tx"; tint: win.cPwr; decimals: 1 }
            }
        }

        // PC host program not connected / socket down: keep the last frame, show a hint.
        Rectangle {
            visible: !panel.linked || !panel.pcConnected
            anchors.centerIn: parent
            width: hint.implicitWidth + 40 * S
            height: hint.implicitHeight + 16 * S
            radius: 8 * S
            color: Qt.rgba(0, 0, 0, 0.7)
            T {
                id: hint
                anchors.centerIn: parent
                text: !panel.linked ? "Connecting to kvmd…" : "Waiting for PC data"
                color: "#ffb454"
                font.pixelSize: 14 * S
            }
        }
    }
}
