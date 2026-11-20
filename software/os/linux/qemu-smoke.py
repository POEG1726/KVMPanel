#!/usr/bin/env python3
"""Boot the KVMPanel initramfs and check login, networking, and USB storage."""

import argparse
import os
from pathlib import Path
import re
import select
import subprocess
import tempfile
import time


def read_until(guest, log, pattern, timeout):
    deadline = time.monotonic() + timeout
    output = bytearray()
    while remaining := max(0, deadline - time.monotonic()):
        if not select.select([guest.stdout], [], [], remaining)[0]:
            break
        chunk = os.read(guest.stdout.fileno(), 4096)
        if not chunk:
            raise RuntimeError("QEMU exited before the serial check finished")
        log.write(chunk)
        output.extend(chunk)
        if match := re.search(pattern, output):
            return match
    raise RuntimeError(f"Timed out waiting for serial output matching {pattern!r}")


def send(guest, command):
    guest.stdin.write(command.encode() + b"\n")
    guest.stdin.flush()


def check(guest, log, label, command):
    # Split the marker in the printf arguments so the serial echo cannot match it.
    send(guest, command + "; printf '\\n%s%s:%d\\n' KVM_ CHECK \"$?\"")
    status = read_until(guest, log, rb"\r?\nKVM_CHECK:(\d+)\r?\n", 15)
    if status[1] != b"0":
        raise RuntimeError(f"{label} failed in the guest (exit {status[1].decode()})")
    print(f"PASS {label}")


def boot(kernel, initramfs, log, diagnostics):
    marker = b"KVMPanel USB smoke\n"
    with tempfile.TemporaryDirectory(prefix="kvmpanel-qemu-") as temp:
        disk = Path(temp) / "usb.img"
        disk.write_bytes(marker.ljust(512 * 1024, b"\0"))
        command = [
            "qemu-system-arm", "-machine", "virt,highmem=off", "-cpu", "cortex-a7",
            "-smp", "2", "-m", "512M", "-nodefaults", "-display", "none",
            "-serial", "stdio", "-monitor", "none", "-no-reboot",
            "-kernel", str(kernel), "-initrd", str(initramfs),
            "-append", "console=ttyAMA0 rdinit=/init",
            "-netdev", "user,id=lan,restrict=on",
            "-device", "virtio-net-device,netdev=lan",
            "-device", "qemu-xhci",
            "-drive", f"if=none,id=usb,file={disk},format=raw,readonly=on",
            "-device", "usb-storage,drive=usb",
        ]
        with subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                              stderr=subprocess.STDOUT) as guest:
            try:
                read_until(guest, log, rb"kvmpanel login: *", 45)
                send(guest, "root")
                read_until(guest, log, rb"\r?\n# ", 10)
                check(guest, log, "root login and two CPUs",
                      'test "$(id -u)" = 0 && test "$(cat /sys/devices/system/cpu/online)" = 0-1')
                check(guest, log, "DHCP and virtual gateway",
                      "ip link set eth0 up && udhcpc -n -q -t 3 -T 1 -i eth0 "
                      "&& ping -q -c 1 -W 3 10.0.2.2")
                wait_for_disk = "for n in 1 2 3 4 5; do test -b /dev/sda && break; sleep 1; done"
                read_marker = f"test \"$(head -c {len(marker)} /dev/sda)\" = '{marker.decode().strip()}'"
                check(guest, log, "USB mass-storage read", f"{wait_for_disk}; {read_marker}")
                if diagnostics:
                    check(guest, log, "diagnostic commands",
                          "ethtool -i eth0 && lsusb && mtdinfo --version && lsmtd --version")
                    check(guest, log, "diagnostic memory test", "memtester 1M 1")
                send(guest, "poweroff")
                read_until(guest, log, rb"Power down", 15)
                if guest.wait(timeout=5) != 0:
                    raise RuntimeError("QEMU reported an error at shutdown")
            finally:
                if guest.poll() is None:
                    guest.kill()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("kernel", type=Path)
    parser.add_argument("initramfs", type=Path)
    parser.add_argument("--log", type=Path, default=Path("qemu-boot.log"))
    parser.add_argument("--diagnostics", action="store_true", help="check the diagnostic initramfs")
    args = parser.parse_args()
    for image in (args.kernel, args.initramfs):
        if not image.is_file():
            parser.error(f"image not found: {image}")
    with args.log.open("wb") as log:
        try:
            boot(args.kernel, args.initramfs, log, args.diagnostics)
        except (RuntimeError, OSError, subprocess.TimeoutExpired) as error:
            parser.exit(1, f"{error}. Serial log: {args.log}\n")
    print(f"PASS shutdown. Serial log: {args.log}")


if __name__ == "__main__":
    main()
