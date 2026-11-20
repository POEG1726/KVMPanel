#!/bin/sh
# PID 1 for the dummy_hcd smoke. All disks here live in the guest's RAM.
set -eu
export PATH=/sbin:/bin:/usr/sbin:/usr/bin
mount -t devtmpfs devtmpfs /dev
mount -t proc proc /proc
mount -t sysfs sysfs /sys
mount -t tmpfs tmpfs /run
trap 'poweroff -f' EXIT
export UDC=dummy_udc.0
g=/sys/kernel/config/usb_gadget/kvmpanel
for slot in cdrom disk shared; do
	image=/tmp/$slot.img
	dd if=/dev/zero of="$image" bs=2048 count=512 2>/dev/null
	printf '%s\n' "$slot" | dd of="$image" conv=notrunc 2>/dev/null
done
export CDROM_IMAGE=/tmp/cdrom.img DISK_IMAGE=/tmp/disk.img SHARE_IMAGE=/tmp/shared.img

wait_for() {
	for n in 1 2 3 4 5 6 7 8 9 10; do
		if "$@"; then return 0; fi
		sleep 1
	done
	echo "Timed out: $*" >&2
	return 1
}

kvmpanel-gadget start
configured() { [ "$(cat "/sys/class/udc/$UDC/state")" = configured ]; }
find_device() {
	for product in /sys/bus/usb/devices/*/product; do
		if [ "$(cat "$product")" = KVMPanel ]; then usb=${product%/product}; return 0; fi
	done
	return 1
}
wait_for find_device
wait_for configured
# Match this gadget, its bound drivers, subordinate hidraw devices and SCSI LUNs.
check_interfaces() {
	hids=0
	stores=0
	inputs=0
	outputs=0
	for intf in "$usb":*; do
		[ -L "$intf/driver" ] || return 1
		case "$(basename "$(readlink "$intf/driver")")" in
		usbhid)
			set -- "$intf"/*/hidraw/hidraw*
			[ -c "/dev/${1##*/}" ] || return 1
			hids=$((hids+1))
			if [ "$(cat "$intf/bInterfaceSubClass")" = 00 ]; then telemetry=/dev/${1##*/}; fi
			;;
		usb-storage) stores=$((stores+1)); store_intf=$intf ;;
		*) return 1 ;;
		esac
		for ep in "$intf"/ep_*/direction; do
			case "$(cat "$ep")" in in) inputs=$((inputs+1)) ;; out) outputs=$((outputs+1)) ;; esac
		done
	done
	[ "$hids:$stores:$inputs:$outputs" = 3:1:4:1 ]
}
wait_for check_interfaces
echo "PASS 3 host HID interfaces and 1 host storage interface, 4 IN / 1 OUT"

check_luns() {
	seen=0
	for node in "$store_intf"/host*/target*/*/block/*; do
		[ -d "$node" ] || return 1
		address=${node%/block/*}
		case ${address##*:} in
		0) expected=cdrom; type=5 ;;
		1) expected=disk; type=0 ;;
		2) expected=shared; type=0 ;;
		*) return 1 ;;
		esac
		[ "$(cat "$address/type")" = "$type" ] || return 1
		[ "$(cat "$node/ro")" = 1 ] || return 1
		[ "$(head -c "${#expected}" "/dev/${node##*/}")" = "$expected" ] || return 1
		seen=$((seen+1))
	done
	[ "$seen" = 3 ]
}
wait_for check_luns
echo "PASS CD-ROM, disk and shared LUN read from host"

# hidraw write falls back to EP0 SET_REPORT when there is no interrupt OUT.
minor=$(cat "$g/functions/hid.absolute/dev")
report=/tmp/report
{ printf '\003KVM'; dd if=/dev/zero bs=60 count=1 2>/dev/null; } > "$report"
dd if="$report" of="$telemetry" bs=64 count=1 2>/dev/null
dd if="/dev/hidg${minor##*:}" of=/tmp/received bs=64 count=1 2>/dev/null
cmp "$report" /tmp/received
echo "PASS host vendor HID output report received over EP0"

if NETWORK=ncm kvmpanel-gadget start; then exit 1; fi
[ "$(cat "$g/UDC")" = "$UDC" ]
kvmpanel-gadget stop
[ ! -e "$g" ]
echo "PASS budget rejection and teardown"

# Exercise the optional functions within USB0's budget, including fresh binds.
unset CDROM_IMAGE DISK_IMAGE SHARE_IMAGE
for mode in ncm rndis; do
	NETWORK=$mode MERGE_MICE=1 kvmpanel-gadget start
	wait_for configured
	[ -e "$g/configs/c.1/$mode.usb0" ]
	[ "$(cat "$g/UDC")" = "$UDC" ]
	kvmpanel-gadget stop
done
REL_MOUSE=0 ABS_MOUSE=0 ACM=1 kvmpanel-gadget start
wait_for configured
[ -d "$g/functions/acm.console" ]
[ -d "$g/functions/hid.telemetry" ]
kvmpanel-gadget stop
echo "PASS merged mice, NCM, RNDIS, ACM and standalone telemetry bind"
echo "PASS KVMPanel gadget smoke"
