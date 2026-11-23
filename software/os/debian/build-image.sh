#!/bin/bash
set -euo pipefail

if (( $# != 6 )); then
    echo "Usage: $0 ROOTFS.tar U_BOOT.bin KERNEL_OUT BUILDROOT_TARGET WEB_DIST OUTPUT.img" >&2
    exit 2
fi
base=$1
uboot=$2
kernel=$3
apps=$4
amp=${BUILDROOT_AMP_TARGET:-$apps}
dashboard=${KVMPANEL_DASHBOARD_BINARY:-}
web=$5
output=$6
files=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

if [[ -e $output ]]; then
    echo "Output already exists: $output" >&2
    exit 1
fi
if (( EUID != 0 )); then
    echo "Run as root to preserve Debian filesystem ownership." >&2
    exit 1
fi

dtb=sun8i-t113i-kvmpanel-drivers.dtb
for input in "$base" "$uboot" "$kernel/arch/arm/boot/zImage" "$kernel/$dtb" \
    "$apps/usr/sbin/kvmd" "$apps/usr/sbin/kvmpanel-stc-power" \
    "$apps/usr/sbin/kvmpanel-gadget" "$apps/usr/sbin/mdnsd" \
    "$apps/usr/bin/gpioget" "$apps/usr/bin/gpioset" "$web/index.html"; do
    if [[ ! -s $input ]]; then
        echo "Missing build input: $input" >&2
        exit 1
    fi
done

# Keep the Allwinner SPL at sector 16, clear of MBR partitions.
sector_size=512
root_start=8192
root_sectors=$((1536 * 1024 * 1024 / sector_size))
data_start=$((root_start + root_sectors))
image_sectors=$((2048 * 1024 * 1024 / sector_size))
data_sectors=$((image_sectors - data_start))
if [[ $(dd if="$uboot" bs=1 skip=4 count=8 status=none) != eGON.BT0 ]] ||
    (( $(stat -c %s "$uboot") > (root_start - 16) * sector_size )); then
    echo "Expected a fitting Allwinner u-boot-sunxi-with-spl.bin." >&2
    exit 1
fi

scratch=$(mktemp -d "$(dirname -- "$output")/.kvmpanel-image.XXXXXX")
trap 'rm -rf -- "$scratch"' EXIT
root=$scratch/root
data=$scratch/data
image=$scratch/sd.img
mkdir -p "$root" "$data"
tar --numeric-owner -xf "$base" -C "$root"
for tool in vim.basic xxd; do
    if [[ ! -x $root/usr/bin/$tool ]]; then
        echo "Debian rootfs is missing $tool. Rebuild it with bootstrap.sh." >&2
        exit 1
    fi
done

disk_id=$(od -An -N4 -tx4 /dev/urandom | tr -d '[:space:]')
cat >"$root/etc/fstab" <<EOF
PARTUUID=$disk_id-01 / ext4 defaults,noatime 0 1
PARTUUID=$disk_id-02 /data ext4 defaults,noatime,nodev,nosuid 0 2
/data/sd /mnt/sd none bind,x-systemd.requires-mounts-for=/data 0 0
EOF
cat >"$root/etc/apt/sources.list" <<'EOF'
deb [signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] https://deb.debian.org/debian trixie main
deb [signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] https://deb.debian.org/debian trixie-updates main
deb [signed-by=/usr/share/keyrings/debian-archive-keyring.gpg] https://deb.debian.org/debian-security trixie-security main
EOF
printf 'kvmpanel\n' >"$root/etc/hostname"
printf '127.0.0.1 localhost\n127.0.1.1 kvmpanel\n::1 localhost ip6-localhost\n' >"$root/etc/hosts"
printf 'uninitialized\n' >"$root/etc/machine-id"
printf 'LANG=C.UTF-8\n' >"$root/etc/locale.conf"
ln -sfn /run/systemd/resolve/stub-resolv.conf "$root/etc/resolv.conf"

install -d "$root/data" "$root/mnt/sd" "$root/mnt/share" "$data/sd"
install -d -m 0700 "$data/kvmd" "$data/kvmd/ssh"
install -Dm0755 "$apps/usr/sbin/kvmd" "$root/usr/sbin/kvmd"
install -Dm0755 "$apps/usr/sbin/kvmpanel-gadget" "$root/usr/sbin/kvmpanel-gadget"
install -Dm0755 "$apps/usr/sbin/kvmpanel-stc-power" "$root/usr/libexec/kvmpanel/stc-power"
install -Dm0755 "$apps/usr/sbin/mdnsd" "$root/usr/sbin/mdnsd"
install -Dm0755 "$apps/usr/bin/gpioget" "$root/usr/libexec/kvmpanel/gpioget"
install -Dm0755 "$apps/usr/bin/gpioset" "$root/usr/libexec/kvmpanel/gpioset"
if [[ -e $amp/usr/bin/kvmpanel-amp-ping ]]; then
    install -Dm0755 "$amp/usr/bin/kvmpanel-amp-ping" "$root/usr/bin/kvmpanel-amp-ping"
    for firmware in c906-echo.elf hifi4-echo.elf; do
        install -Dm0644 "$amp/lib/firmware/kvmpanel/$firmware" "$root/usr/lib/firmware/kvmpanel/$firmware"
    done
fi
install -d "$root/usr/share/kvmd/web"
cp -a --no-preserve=ownership "$web/." "$root/usr/share/kvmd/web/"

install -Dm0755 "$files/../buildroot/package/kvmd/S95kvmd" "$root/usr/libexec/kvmd/run-service"
install -Dm0755 "$files/kvmd-control.sh" "$root/etc/init.d/S95kvmd"
install -Dm0755 "$files/device-service.sh" "$root/usr/libexec/kvmd/dropbear"
install -Dm0755 "$files/device-service.sh" "$root/usr/libexec/kvmd/mdnsd"
install -Dm0755 "$files/stc-control.sh" "$root/usr/sbin/kvmpanel-stc-power"
install -Dm0755 "$files/stc-shutdown.sh" "$root/usr/lib/systemd/system-shutdown/kvmpanel-stc"
if [[ -d $root/etc/dropbear ]]; then
    cp -a "$root/etc/dropbear/." "$data/kvmd/ssh/"
    rm -rf -- "$root/etc/dropbear"
fi
ln -s /data/kvmd/ssh "$root/etc/dropbear"

cat >"$root/etc/default/kvmd" <<'EOF'
KVMD_GPIO_CHIP=2000000.pinctrl
KVMD_GPIO_POWER=141
KVMD_GPIO_RESET=131
KVMD_GPIO_POWER_LED=140
EOF
install -Dm0644 "$files/firstboot.conf" "$root/etc/systemd/system/systemd-firstboot.service.d/kvmpanel.conf"
install -Dm0644 "$files/eth0.network" "$root/etc/systemd/network/20-eth0.network"
install -m 0644 "$files/"*.service "$root/etc/systemd/system/"
systemctl --root="$root" enable kvmd.service systemd-networkd.service systemd-resolved.service
if [[ -n $dashboard ]]; then
    install -Dm0755 "$dashboard" "$root/usr/bin/kvmpanel-dashboard"
    systemctl --root="$root" enable kvmpanel-dashboard.service
fi

# The first cold-boot profile stays at 3.3 V. Preserve the tested DT build
# with the UHS modes as a second menu entry for bring-up.
install -Dm0644 "$kernel/arch/arm/boot/zImage" "$root/boot/zImage"
install -Dm0644 "$kernel/$dtb" "$root/boot/$dtb"
hs=${dtb%.dtb}-sd-hs.dtb
cp "$root/boot/$dtb" "$root/boot/$hs"
mmc=/soc/mmc@4020000
for mode in sdr12 sdr25 sdr50 sdr104 ddr50; do
    if fdtget -p "$root/boot/$hs" "$mmc" | grep -xq "sd-uhs-$mode"; then
        fdtput -d "$root/boot/$hs" "$mmc" "sd-uhs-$mode"
    fi
done
fdtput "$root/boot/$hs" "$mmc" no-1-8-v
fdtput -t u "$root/boot/$hs" "$mmc" max-frequency 50000000
mkdir -p "$root/boot/extlinux"
cat >"$root/boot/extlinux/extlinux.conf" <<EOF
DEFAULT debian
TIMEOUT 30
MENU TITLE KVMPanel SD boot

LABEL debian
  MENU LABEL Debian 13 (SD High Speed, 3.3V)
  LINUX /boot/zImage
  FDT /boot/$hs
  APPEND root=PARTUUID=$disk_id-01 rootwait ro console=ttyS0,115200 net.ifnames=0

LABEL debian-uhs
  MENU LABEL Debian 13 (UHS, for board testing)
  LINUX /boot/zImage
  FDT /boot/$dtb
  APPEND root=PARTUUID=$disk_id-01 rootwait ro console=ttyS0,115200 net.ifnames=0
EOF

# Use separate regular files for formatting. The host needs no loop devices.
truncate -s "$((image_sectors * sector_size))" "$image"
sfdisk "$image" <<EOF
label: dos
label-id: 0x$disk_id
unit: sectors
start=$root_start, size=$root_sectors, type=83, bootable
start=$data_start, size=$data_sectors, type=83
EOF
for name in root data; do
    start=$root_start
    sectors=$root_sectors
    if [[ $name == data ]]; then
        start=$data_start
        sectors=$data_sectors
    fi
    part="$scratch/$name.ext4"
    truncate -s "$((sectors * sector_size))" "$part"
    mkfs.ext4 -q -F -L "kvmpanel-$name" -O ^64bit,^metadata_csum_seed,^orphan_file \
        -d "$scratch/$name" "$part"
    e2fsck -fn "$part"
    dd if="$part" of="$image" bs=1M seek="$((start / 2048))" conv=sparse,notrunc status=none
done
dd if="$uboot" of="$image" bs="$sector_size" seek=16 conv=notrunc status=none
sfdisk --verify "$image"
chmod 0644 "$image"
ln -- "$image" "$output"
