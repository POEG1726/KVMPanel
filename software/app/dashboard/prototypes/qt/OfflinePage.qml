// PC-offline page (spec 4.2, mockup landscape-offline.html part 2): the board's own status while the
// PC host program sends nothing. Portrait 1080x1920 stacks the tiles; landscape 1920x1080 puts the
// big state (2 columns) and Remote access on row 1, then Network / KVM / board on row 2.
import QtQuick
import QtQuick.Shapes

Item {
    id: page
    property bool landscape: false
    property int countdown: -1          // seconds until screen off, -1 = never
    signal powerOnRequested()

    readonly property color teal: "#7dd3c8"
    readonly property var b: panel.board
    readonly property bool has: b && b.t === "board"
    readonly property string power: has ? b.pc_power : "unknown"
    readonly property bool pcOff: power === "off"

    // ---- formatting -------------------------------------------------------------
    function ago(ms) {
        if (isNaN(ms) || ms < 0) return "no data yet"
        const s = Math.round(ms / 1000)
        if (s < 60) return "last data " + s + " s ago"
        if (s < 3600) return "last data " + Math.round(s / 60) + " min ago"
        if (s < 86400) return "last data " + Math.round(s / 3600) + " h ago"
        return "last data " + Math.round(s / 86400) + " d ago"
    }
    function rate(mbs) { return mbs < 1 ? Math.round(mbs * 1000) + " KB/s" : mbs.toFixed(1) + " MB/s" }
    function gb(bytes) { return (bytes / 1e9).toFixed(1) }
    function uptime(s) {
        const d = Math.floor(s / 86400), h = Math.floor(s % 86400 / 3600), m = Math.floor(s % 3600 / 60)
        return d > 0 ? d + " d " + String(h).padStart(2, "0") + " h" : h + " h " + String(m).padStart(2, "0") + " min"
    }
    function usbNames(list) {
        const names = { keyboard: "Kbd", mouse_rel: "Mouse", mouse_abs: "Mouse", storage: "Disk", net: "Net", acm: "Serial", telemetry: "Telemetry" }
        const out = []
        for (const f of list) { const n = names[f] || f; if (out.indexOf(n) < 0) out.push(n) }
        return out.length ? out.join(" · ") : "None"
    }
    function sessions(list) {
        if (!list.length) return "None"
        const first = list[0].user + " @ " + list[0].ip
        return list.length + " · " + first + (list.length > 1 ? " +" + (list.length - 1) : "")
    }

    // ---- layout -----------------------------------------------------------------
    readonly property real pad: 9.24 * S
    readonly property real gap: 7 * S
    readonly property real lw: landscape ? 66 * S : 0     // landscape: fixed label column, sparklines stretch

    Item {
        id: top
        x: page.pad; y: page.pad
        width: page.width - 2 * page.pad
        height: Math.max(brand.implicitHeight, bar.implicitHeight) + 2 * S
        Row {
            id: brand
            anchors.verticalCenter: parent.verticalCenter
            spacing: 6 * S
            T { id: brandName; text: "KVMPanel"; font.weight: Font.DemiBold; font.pixelSize: 13 * S; font.letterSpacing: 0.5 * S }
            T {   // dropped when it would run into the chips (portrait); Remote access shows the name anyway
                visible: brandName.implicitWidth + 6 * S + implicitWidth + 8 * S + bar.implicitWidth <= top.width
                text: page.has ? page.b.hostname + ".local" : ""
                color: "#8b97aa"; font.pixelSize: 10 * S
            }
        }
        StatusBar { id: bar; offline: true; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter }
    }

    Item {
        id: foot
        x: page.pad + 4 * S
        width: page.width - 2 * x
        y: page.height - page.pad - height
        height: cd.implicitHeight
        T {
            id: cd
            color: "#8b97aa"; font.pixelSize: 10 * S
            textFormat: Text.StyledText
            text: page.countdown < 0 ? "Screen stays on"
                : "Screen off in <font color='#c5cfdc'><b>" + Math.floor(page.countdown / 60) + ":" + String(page.countdown % 60).padStart(2, "0") + "</b></font>"
        }
        T { anchors.right: parent.right; text: "FN · Settings"; color: "#8b97aa"; font.pixelSize: 10 * S }
    }

    // Content area between the top bar and the footer.
    Item {
        id: body
        x: page.pad
        y: top.y + top.height + page.gap
        width: page.width - 2 * page.pad
        height: foot.y - page.gap - y

        readonly property real colW: (width - 2 * page.gap) / 3
        readonly property real row1: Math.max(hero.implicitHeight, access.implicitHeight)
        // Portrait: hero and access at content height, the rest split 1 : 1.25 : 1.25.
        readonly property real flexUnit: (height - hero.implicitHeight - access.implicitHeight - 4 * page.gap) / 3.5

        // ---------------- Big state ----------------
        Tile {
            id: hero
            x: 0; y: 0
            width: page.landscape ? 2 * body.colW + page.gap : body.width
            height: page.landscape ? body.row1 : implicitHeight
            Item {
                width: parent.width
                height: Math.max(icon.height, stateCol.implicitHeight, btn.height) + 6 * S
                Item {
                    id: icon
                    width: (page.landscape ? 54 : 60) * S; height: width
                    anchors.verticalCenter: parent.verticalCenter
                    Shape {
                        anchors.fill: parent
                        ShapePath {
                            scale: Qt.size(icon.width / 24, icon.height / 24)
                            strokeColor: page.pcOff ? "#6b7585" : "#ffb454"
                            strokeWidth: 1.6 * icon.width / 24
                            fillColor: "transparent"
                            capStyle: ShapePath.RoundCap
                            joinStyle: ShapePath.RoundJoin
                            PathSvg { path: "M4.5 4H19.5Q21 4 21 5.5V14.5Q21 16 19.5 16H4.5Q3 16 3 14.5V5.5Q3 4 4.5 4ZM8 20h8M12 16v4M12 7.5v3M9.8 8.7A3 3 0 1 0 14.2 8.7" }
                        }
                    }
                    Shape {   // spinner ring while waiting for the host program
                        visible: !page.pcOff
                        anchors.centerIn: parent
                        width: parent.width + 12 * S; height: width
                        ShapePath {
                            strokeColor: "#ffb454"
                            strokeWidth: 2 * S
                            fillColor: "transparent"
                            capStyle: ShapePath.RoundCap
                            PathAngleArc {
                                centerX: icon.width / 2 + 6 * S; centerY: centerX
                                radiusX: centerX - 1 * S; radiusY: radiusX
                                startAngle: -90; sweepAngle: 90
                            }
                        }
                        RotationAnimation on rotation {
                            running: !page.pcOff; loops: Animation.Infinite
                            from: 0; to: 360; duration: 1400
                        }
                    }
                }
                Column {
                    id: stateCol
                    anchors.left: icon.right
                    anchors.leftMargin: 14 * S
                    anchors.right: btn.visible ? btn.left : parent.right
                    anchors.rightMargin: 8 * S
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 2 * S
                    T {
                        width: parent.width; elide: Text.ElideRight
                        text: page.pcOff ? "PC is off" : "Waiting for host app"
                        font.pixelSize: 20.6 * S; font.weight: Font.DemiBold
                    }
                    T {
                        width: parent.width; elide: Text.ElideRight
                        color: "#8b97aa"; font.pixelSize: 10.6 * S
                        text: {
                            const when = page.ago(page.has ? page.b.ts - panel.lastSeenTs : NaN)
                            if (page.pcOff) return "Power LED off · " + when
                            if (page.power === "unknown") return "Power LED unknown · " + when
                            return (panel.pcLink === "off" ? "PC is on, USB not connected · " : "PC is on, USB connected · ") + when
                        }
                    }
                }
                // Shown only while the PC is off, so a press can never switch it off.
                Rectangle {
                    id: btn
                    visible: page.pcOff
                    focus: visible
                    activeFocusOnTab: true
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: btnRow.implicitWidth + 20 * S
                    height: btnRow.implicitHeight + 12 * S
                    radius: 7 * S
                    color: btnArea.pressed ? Qt.rgba(126 / 255, 231 / 255, 135 / 255, 0.15) : "transparent"
                    border.width: activeFocus ? 2 * S : Math.max(1, Math.round(0.8 * S))
                    border.color: activeFocus ? "#2f6fd6" : Qt.rgba(126 / 255, 231 / 255, 135 / 255, 0.45)
                    readonly property color fg: panel.powerOn === "failed" ? "#ffb454" : panel.powerOn === "pending" ? "#8b97aa" : "#7ee787"
                    Row {
                        id: btnRow
                        anchors.centerIn: parent
                        spacing: 5 * S
                        Shape {
                            width: 11 * S; height: width
                            anchors.verticalCenter: parent.verticalCenter
                            ShapePath {
                                scale: Qt.size(11 * S / 12, 11 * S / 12)
                                strokeColor: btn.fg; strokeWidth: 1.6 * S; fillColor: "transparent"
                                capStyle: ShapePath.RoundCap
                                PathSvg { path: "M6 1.5v4M3.4 3.2A4 4 0 1 0 8.6 3.2" }
                            }
                        }
                        T {
                            color: btn.fg; font.pixelSize: 10.6 * S
                            text: panel.powerOn === "pending" ? "Powering on…" : panel.powerOn === "failed" ? "Refused" : "Power on PC"
                        }
                    }
                    MouseArea { id: btnArea; anchors.fill: parent; onClicked: page.powerOnRequested() }
                    Keys.onReturnPressed: page.powerOnRequested()
                    Keys.onSpacePressed: page.powerOnRequested()
                }
            }
        }

        // ---------------- Remote access ----------------
        Tile {
            id: access
            x: page.landscape ? 2 * (body.colW + page.gap) : 0
            y: page.landscape ? 0 : hero.height + page.gap
            width: page.landscape ? body.colW : body.width
            height: page.landscape ? body.row1 : implicitHeight
            Title { text: "Remote access" }
            T {
                text: panel.ip || "No network"
                color: page.teal
                font.pixelSize: (page.landscape ? 28.75 : 32.5) * S
                font.weight: Font.DemiBold
            }
            T { text: page.has ? page.b.access.url : "--"; color: "#c5cfdc"; font.pixelSize: 14.4 * S }
            KvRow {
                visible: page.has && page.b.access.tailscale_ip !== null
                label: "Tailscale"; value: page.has && page.b.access.tailscale_ip ? page.b.access.tailscale_ip : ""
            }
        }

        // ---------------- Network ----------------
        Tile {
            id: net
            x: 0
            y: page.landscape ? body.row1 + page.gap : access.y + access.height + page.gap
            width: page.landscape ? body.colW : body.width
            height: page.landscape ? body.height - y : body.flexUnit
            Title { text: page.has ? "Network · " + page.b.eth.iface : "Network" }
            KvRow {
                label: "Link"
                dim: !page.has || page.b.eth.link_mbps === 0
                value: {
                    if (!page.has) return "--"
                    const e = page.b.eth
                    if (e.link_mbps === 0) return "Down"
                    const sp = e.link_mbps >= 1000 ? e.link_mbps / 1000 + " Gbps" : e.link_mbps + " Mbps"
                    return e.duplex ? sp + " · " + e.duplex + " duplex" : sp
                }
            }
            MetricRow { metric: "board.rx"; tint: page.teal; labelWidth: page.lw; fmt: page.rate }
            MetricRow { metric: "board.tx"; tint: page.teal; labelWidth: page.lw; fmt: page.rate }
            KvRow { label: "MAC"; value: page.has ? page.b.eth.mac : "--" }
        }

        // ---------------- KVM ----------------
        Tile {
            id: kvm
            x: page.landscape ? body.colW + page.gap : 0
            y: page.landscape ? net.y : net.y + net.height + page.gap
            width: page.landscape ? body.colW : body.width
            height: page.landscape ? net.height : 1.25 * body.flexUnit
            readonly property var k: page.has ? page.b.kvm : null
            Title { text: "KVM" }
            KvRow { label: "Sessions"; dim: !kvm.k || !kvm.k.sessions.length; value: kvm.k ? page.sessions(kvm.k.sessions) : "--" }
            KvRow {
                label: "HDMI in"
                dim: !kvm.k || !kvm.k.hdmi.signal
                value: !kvm.k ? "--" : kvm.k.hdmi.signal ? kvm.k.hdmi.width + "×" + kvm.k.hdmi.height + " @ " + Math.round(kvm.k.hdmi.fps) : "No signal"
            }
            KvRow {
                label: "Stream"
                dim: !kvm.k || kvm.k.stream.state !== "streaming"
                value: !kvm.k ? "--" : kvm.k.stream.state === "streaming" ? Math.round(kvm.k.stream.fps) + " fps · MJPEG" : "Idle"
            }
            KvRow {
                label: "Media"
                dim: !kvm.k || kvm.k.media.mounted === null
                value: !kvm.k ? "--" : kvm.k.media.mounted === null ? "None" : kvm.k.media.mounted + (kvm.k.media.mode === "cdrom" ? " · CD" : "")
            }
            KvRow { label: "USB"; value: kvm.k ? page.usbNames(kvm.k.usb_functions) : "--" }
        }

        // ---------------- The board itself ----------------
        Tile {
            id: brd
            x: page.landscape ? 2 * (body.colW + page.gap) : 0
            y: page.landscape ? net.y : kvm.y + kvm.height + page.gap
            width: page.landscape ? body.colW : body.width
            height: page.landscape ? net.height : body.height - y
            Title { text: "KVMPanel · T113-i" }
            MetricRow { metric: "board.soc"; tint: page.teal; labelWidth: page.lw; decimals: 1 }
            MetricRow { metric: "board.cpu"; tint: page.teal; labelWidth: page.lw }
            MetricRow { metric: "board.mem"; tint: page.teal; labelWidth: page.lw; ofMax: true }
            Bar { metric: "board.mem"; tint: page.teal }
            KvRow { label: "SD card"; value: page.has && page.b.sd ? page.gb(page.b.sd.used) + " / " + page.gb(page.b.sd.total) + " GB" : "No card"; dim: !page.has || !page.b.sd }
            KvRow { label: "Uptime / ver"; value: page.has ? page.uptime(page.b.uptime_s) + " · " + page.b.version : "--" }
        }
    }

    component Title: T {
        color: page.teal
        font.pixelSize: 10 * S
        font.letterSpacing: 0.6 * S
        font.capitalization: Font.AllUppercase
    }
}
