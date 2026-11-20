#!/bin/sh
# Run USB host and gadget within one QEMU guest using Linux dummy_hcd.
set -eu
if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
	echo "usage: $0 zImage rootfs.cpio.gz [serial.log]" >&2
	exit 2
fi
kernel=$1
rootfs=$2
log=${3:-qemu-gadget.log}
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp "$(dirname "$0")/qemu-gadget-init.sh" "$tmp/init-gadget-smoke"
chmod 755 "$tmp/init-gadget-smoke"
# Add only the test init. The gadget executable must already be in the rootfs.
(cd "$tmp" && printf '%s\n' init-gadget-smoke | cpio --quiet -o -H newc --owner 0:0) > "$tmp/test.cpio"
cat "$rootfs" > "$tmp/initrd"
gzip -c "$tmp/test.cpio" >> "$tmp/initrd"
if ! timeout 120 qemu-system-arm -machine virt,highmem=off -cpu cortex-a7 -smp 2 -m 512M \
	-nodefaults -display none -monitor none -serial stdio -no-reboot \
	-kernel "$kernel" -initrd "$tmp/initrd" -append 'console=ttyAMA0 rdinit=/init-gadget-smoke panic=1' \
	< /dev/null > "$log" 2>&1; then
	tail -30 "$log" >&2
	exit 1
fi
if ! grep -q '^PASS KVMPanel gadget smoke' "$log"; then tail -40 "$log" >&2; exit 1; fi
grep '^PASS ' "$log"
