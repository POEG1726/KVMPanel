// ETH / PC / USB chips shared by the main screen and the offline page.
// `offline`: the host program is known to be missing, so a partial PC link means HID only.
import QtQuick

Row {
    property bool offline: false
    spacing: 5 * S
    Chip {
        level: panel.ethState; label: panel.ethText
        icon: "M3 3H9Q10 3 10 4V8Q10 9 9 9H3Q2 9 2 8V4Q2 3 3 3ZM4 9v1.5M6 9v1.5M8 9v1.5"
    }
    Chip {
        level: panel.pcState
        label: parent.offline && panel.pcLink === "partial" ? "PC HID only" : panel.pcText
        icon: "M2.5 2H9.5Q10.5 2 10.5 3V7Q10.5 8 9.5 8H2.5Q1.5 8 1.5 7V3Q1.5 2 2.5 2ZM4 10.5h4M6 8v2.5"
    }
    Chip {
        level: panel.usbState; label: panel.usbText
        icon: "M6 1v8M4 3l2-2 2 2M3 6v1.5l3 1.5M9 5v1.5L6 8"
    }
}
