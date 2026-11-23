#!/bin/sh
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "Usage: $0 ROOTFS.tar [debian-archive-keyring.gpg]" >&2
    exit 2
fi
output=$1
archive_keyring=${2:-/usr/share/keyrings/debian-archive-keyring.gpg}

if [ -e "$output" ]; then
    echo "Output already exists: $output" >&2
    exit 1
fi

# A standalone signed-by keyring works on Ubuntu hosts as well as Debian.
keyring=$(mktemp /tmp/kvmpanel-debian-keyring.XXXXXX.gpg)
temp_tar=$(mktemp "$output.XXXXXX")
trap 'rm -f -- "$keyring" "$temp_tar"' EXIT
install -m 0644 "$archive_keyring" "$keyring"

packages=systemd-sysv,libpam-systemd,dbus,systemd-resolved,systemd-timesyncd,udev
packages=$packages,iproute2,kmod,ca-certificates,vim,dropbear-bin,gpiod,v4l-utils
packages=$packages,ethtool,usbutils,i2c-tools,curl,e2fsprogs,xxd

mmdebstrap --format=tar --mode=root --variant=minbase --architectures=armhf \
    --keyring="$keyring" --include="$packages" trixie "$temp_tar" \
    "deb [signed-by=$keyring] https://deb.debian.org/debian trixie main" \
    "deb [signed-by=$keyring] https://deb.debian.org/debian trixie-updates main" \
    "deb [signed-by=$keyring] https://deb.debian.org/debian-security trixie-security main"
chmod 0644 "$temp_tar"
ln -- "$temp_tar" "$output"
