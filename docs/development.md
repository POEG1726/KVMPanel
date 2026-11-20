# 开发说明

## 文件放哪里

主仓库在 `D:\MyProj\KVMPanel`，上游镜像在 `sources/repos/`。Ubuntu 24.04 WSL 的工作副本位于 `~/work/KVMPanel/upstream/`。在 WSL 的 Linux 文件系统构建，保留源码的大小写和符号链接。

自己的设备树、配置和补丁放 `software/os/`，应用放 `software/app/`，STC 固件放 `software/firmware/stc8/`。构建输出放 `~/work/KVMPanel/build/`，不混进源码。

## WSL 编译环境

本机内核构建使用 ARM hard-float GCC 13.3 和 binutils 2.42。Buildroot 单独下载它所需的 Bootlin ARMv7 hard-float musl 工具链。

```sh
sudo apt-get update
sudo apt-get install build-essential gcc-arm-linux-gnueabihf \
  binutils-arm-linux-gnueabihf bison flex libssl-dev libelf-dev bc cpio \
  rsync unzip file wget patch python3 pkg-config libncurses-dev \
  device-tree-compiler u-boot-tools libgnutls28-dev python3-dev \
  python3-setuptools python3-pyelftools swig uuid-dev
```

## 构建板级内核和设备树

以下命令在 WSL 中执行。DTS 位于本仓库，构建时引用 Linux 的 SoC 描述。

```sh
project=/mnt/d/MyProj/KVMPanel
work="$HOME/work/KVMPanel"
linux="$work/upstream/linux"
kout="$work/build/linux-kvmpanel"
export ARCH=arm CROSS_COMPILE=arm-linux-gnueabihf-

mkdir -p "$kout"
make -C "$linux" O="$kout" sunxi_defconfig
"$linux/scripts/kconfig/merge_config.sh" -m -O "$kout" "$kout/.config" \
  "$project/software/os/linux/configs/kvmpanel.fragment"
make -C "$linux" O="$kout" olddefconfig
make -C "$linux" O="$kout" -j"$(nproc)" zImage

arm-linux-gnueabihf-gcc -E -nostdinc -undef -D__DTS__ -x assembler-with-cpp \
  -I "$linux/scripts/dtc/include-prefixes" \
  -I "$linux/arch/arm/boot/dts/allwinner" \
  "$project/software/os/linux/dts/sun8i-t113i-kvmpanel.dts" \
  -o "$kout/kvmpanel.dts.tmp"
dtc -I dts -O dtb -o "$kout/sun8i-t113i-kvmpanel.dtb" "$kout/kvmpanel.dts.tmp"
```

## 构建四 lane MIPI DSI 主控内核

以下步骤使用 Linux 6.18.64。首次构建时从同一基线建立独立源码目录，应用本板 DSI 补丁。复用此目录时从 `mkdir` 继续。

```sh
dsi_linux="$work/upstream/linux-dsi"
dsi_out="$work/build/linux-kvmpanel-dsi"
linux_files="$project/software/os/linux"
export ARCH=arm CROSS_COMPILE=arm-linux-gnueabihf-

git clone --shared "$linux" "$dsi_linux"
git -C "$dsi_linux" apply "$linux_files/patches"/000[12]-sun6i-dsi-*.patch

mkdir -p "$dsi_out"
make -C "$dsi_linux" O="$dsi_out" sunxi_defconfig
"$dsi_linux/scripts/kconfig/merge_config.sh" -m -O "$dsi_out" "$dsi_out/.config" \
  "$linux_files/configs/kvmpanel.fragment" "$linux_files/configs/kvmpanel-dsi.fragment"
make -C "$dsi_linux" O="$dsi_out" olddefconfig
make -C "$dsi_linux" O="$dsi_out" -j"$(nproc)" zImage

arm-linux-gnueabihf-gcc -E -nostdinc -undef -D__DTS__ -x assembler-with-cpp \
  -I "$dsi_linux/scripts/dtc/include-prefixes" \
  -I "$dsi_linux/arch/arm/boot/dts/allwinner" \
  "$linux_files/dts/sun8i-t113i-kvmpanel-dsi.dts" -o "$dsi_out/kvmpanel-dsi.dts.tmp"
dtc -I dts -O dtb -o "$dsi_out/sun8i-t113i-kvmpanel-dsi.dtb" "$dsi_out/kvmpanel-dsi.dts.tmp"
```

使用这套 `zImage`、`sun8i-t113i-kvmpanel-dsi.dtb` 和下节的基础 initramfs 从 FEL 启动。可先通过串口检查 `dmesg`、`/sys/class/drm`，看 DSI 主控和 DRM 是否注册、是否仍有时钟延期。当前 DTB 没有面板，注册成功时 DSI connector 预期为 `disconnected`。 [面板参考资料和适配状态](linux-6.18.md#mipi-dsi)

## 构建 initramfs

```sh
bout="$work/build/buildroot-kvmpanel"
(
  unset ARCH CROSS_COMPILE LD_LIBRARY_PATH
  export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
  make -C "$work/upstream/buildroot" O="$bout" \
    BR2_EXTERNAL="$project/software/os/buildroot" kvmpanel_defconfig
  make -C "$work/upstream/buildroot" O="$bout"
)
```

产物位于 `$kout/arch/arm/boot/zImage`、`$kout/sun8i-t113i-kvmpanel.dtb` 和 `$bout/images/rootfs.cpio.gz`。Buildroot 使用 BusyBox init，在 `/dev/console` 启动 getty，默认 root 可从串口登录。上板加载方式见 [T113-i 启动链](../software/os/vendor/tina/README.md)。

## 在 QEMU 验证启动

QEMU 使用 `virt` 和两个 Cortex-A7，加载同一份 Buildroot initramfs。测试脚本通过串口登录，检查两核、通过 DHCP 获取虚拟网段地址、探测网关、读取只读 USB 磁盘，最后关机。虚拟网络启用了 `restrict=on`。

```sh
sudo apt-get install --no-install-recommends qemu-system-arm
qout="$work/build/linux-kvmpanel-qemu"
configs="$project/software/os/linux/configs"
export ARCH=arm CROSS_COMPILE=arm-linux-gnueabihf-

mkdir -p "$qout" "$work/logs"
make -C "$linux" O="$qout" sunxi_defconfig
"$linux/scripts/kconfig/merge_config.sh" -m -O "$qout" "$qout/.config" \
  "$configs/kvmpanel.fragment" "$configs/qemu-virt.fragment"
make -C "$linux" O="$qout" olddefconfig
make -C "$linux" O="$qout" -j"$(nproc)" zImage
python3 "$project/software/os/linux/qemu-smoke.py" \
  "$qout/arch/arm/boot/zImage" "$bout/images/rootfs.cpio.gz" \
  --log "$work/logs/kvmpanel-qemu-serial.log"
```

QEMU 的串口、网卡和 USB 控制器使用 PL011、VirtIO 和 xHCI。T113-i 的 DDR 训练、FEL、RTL8211F、MUSB/EHCI/OHCI 和 SPI NAND 需要在实板验证。

## 构建诊断 initramfs

诊断镜像在基础 BusyBox 上加 `memtester`、`ethtool`、`mtdinfo` 和 `lsmtd`。已通过 QEMU 虚拟网口查询、USB 枚举、MTD 工具版本检查和 1 MiB 内存读写测试。可用相同脚本附加 `--diagnostics` 复现。

```sh
br="$work/upstream/buildroot"
external="$project/software/os/buildroot"
dout="$work/build/buildroot-kvmpanel-diagnostics"
(
  unset ARCH CROSS_COMPILE LD_LIBRARY_PATH
  export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
  make -C "$br" O="$dout" BR2_EXTERNAL="$external" kvmpanel_defconfig
  CONFIG_=BR2_ "$br/support/kconfig/merge_config.sh" -m -O "$dout" "$dout/.config" \
    "$external/configs/kvmpanel-diagnostics.fragment"
  make -C "$br" O="$dout" olddefconfig
  make -C "$br" O="$dout"
)
python3 "$project/software/os/linux/qemu-smoke.py" --diagnostics \
  "$qout/arch/arm/boot/zImage" "$dout/images/rootfs.cpio.gz" \
  --log "$work/logs/kvmpanel-qemu-diagnostics.log"
```

## 首次上板

1. 用 USB1 进 FEL，USB2 看 115200 8N1 串口。按 [FEL 加载步骤](../software/os/u-boot/README.md) 从 RAM 进 SPL 和 U-Boot，用 `bdinfo` 核对是否探测到 512 MiB。使用诊断镜像时，把加载步骤里的 `initramfs` 指向 `$dout/images/rootfs.cpio.gz` 并按实际文件重新计算 `rdsize`。
2. 进入 Linux 后确认 root 登录、`cat /sys/devices/system/cpu/online` 为 `0-1`，并用 `cat /proc/meminfo` 和 `memtester 128M 1` 检查内存。记录可用内存和串口输出。
3. 插入测试 U 盘，检查 `lsusb`、`dmesg` 和新出现的块设备，再做只读读取。检查 `/sys/class/udc` 是否有 MUSB 控制器，USB0 的 Linux 枚举需要配置 gadget 功能。
4. 网口和 NAND 分开启用并验证。网口用 `ip link set eth0 up`、`ethtool eth0` 检查链路，再用 `udhcpc -n -q -i eth0` 获取地址并测试收发。NAND 在 RAM 启动后启用 `/soc/spi@4025000`，用 `dmesg`、`cat /proc/mtd` 和 `mtdinfo -a` 检查 W25N04KW 的识别和容量。需要确认实际指令宽度时，在 IO2、IO3 上抓读页波形。

基础外设跑通后，再验证 NAND 的 BROM → Boot0 → U-Boot 链路。按实测结果决定持久启动布局、内核加载方式和根文件系统，并保留 FEL 恢复路径。

## 单独验证本板网口

基础 DTB 指定了 RTL8211F 的地址 1、PG11 上的 25 MHz 参考时钟和 RGMII pinmux。`rgmii-id` 以 PCB 无额外时钟偏移为初始假设，需要结合实际走线验证。首次上板走通串口后，可生成临时 DTB 来测试网口，测量参考时钟和 PHY 复位并检查链路。

```sh
netdt="$kout/sun8i-t113i-kvmpanel-network.dtb"
cp "$kout/sun8i-t113i-kvmpanel.dtb" "$netdt"
fdtput -t s "$netdt" /soc/ethernet@4500000 status okay
```

## 构建应用所需的内核、模块和组合 DTB

在 WSL 使用 Linux 6.18.64、ARM hard-float GCC 和 dtc。命令块使用 Bash。以下命令需要工作区中的基础、DSI 和 QEMU 配置，以及 0001、0002 两个 DSI 补丁。新建工作副本只需执行一次。

```bash
set -e
project=/mnt/d/MyProj/KVMPanel
work="$HOME/work/KVMPanel"
files="$project/software/os/linux"
linux="$work/upstream/linux-plan2"
out="$work/build/linux-plan2"
qout="$work/build/linux-kvmpanel-qemu"
bout="$work/build/buildroot-plan2"
export ARCH=arm CROSS_COMPILE=arm-linux-gnueabihf-

# 仅在新目录执行 clone 和 apply。
git clone --shared --branch v6.18.64 "$work/upstream/linux" "$linux"
for patch in "$files"/patches/000{1,2,3,4}-*.patch; do
    git -C "$linux" apply "$patch"
done
mkdir -p "$out"
make -C "$linux" O="$out" sunxi_defconfig
"$linux/scripts/kconfig/merge_config.sh" -m -O "$out" "$out/.config" \
    "$files/configs/kvmpanel.fragment" "$files/configs/kvmpanel-dsi.fragment" \
    "$files/configs/kvmpanel-gadget.fragment" "$files/configs/kvmpanel-peripherals.fragment" \
    "$files/configs/kvmpanel-capture.fragment" "$files/configs/kvmpanel-display.fragment"
make -C "$linux" O="$out" olddefconfig
make -C "$linux" O="$out" -j12 zImage modules

arm-linux-gnueabihf-gcc -E -nostdinc -undef -D__DTS__ -x assembler-with-cpp \
    -I "$linux/scripts/dtc/include-prefixes" -I "$linux/arch/arm/boot/dts/allwinner" \
    "$files/dts/sun8i-t113i-kvmpanel-drivers.dts" -o "$out/board.pp"
dtc -I dts -O dtb -o "$out/sun8i-t113i-kvmpanel-drivers.dtb" "$out/board.pp"
for patch in "$files"/patches/000{3,4}-*.patch; do
    "$linux/scripts/checkpatch.pl" --strict --ignore FILE_PATH_CHANGES "$patch"
done
```

此配置把板级驱动编进 zImage，initramfs 可直接启动。要单独验证模块链接，复制上述 `.config` 到独立输出目录，用 `scripts/config` 改为模块后执行完整构建。

```bash
mout="$work/build/linux-plan2-modules"
mkdir -p "$mout"
cp "$out/.config" "$mout/.config"
"$linux/scripts/config" --file "$mout/.config" -m VIDEO_LT8619C -m VIDEO_SUN20I_CSI -m DRM_PANEL_TD4322
make -C "$linux" O="$mout" olddefconfig
make -C "$linux" O="$mout" -j12 zImage modules
find "$mout/drivers" -name 'lt8619c.ko' -o -name 'sun20i-csi.ko' -o -name 'panel-td4322.ko'
```

## Buildroot 与 QEMU 的组合验证

Buildroot 2025.02.20 的板级 defconfig 生成带 NEON libjpeg-turbo、v4l2-ctl、media-ctl、evtest、libgpiod 1.x、i2c-tools 和 `/usr/sbin/kvmpanel-gadget` 的 initramfs。镜像和共享盘的路径由 gadget 调用方传入。以下命令从板级配置开始，适用于只读保留旧 defconfig 的工作区；已经合入新版 defconfig 时可省去 merge 和 olddefconfig 两步。

```bash
(
    unset ARCH CROSS_COMPILE LD_LIBRARY_PATH
    export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
    br="$work/upstream/buildroot"
    ext="$project/software/os/buildroot"
    make -C "$br" O="$bout" BR2_EXTERNAL="$ext" kvmpanel_defconfig
    CONFIG_=BR2_ "$br/support/kconfig/merge_config.sh" -m -O "$bout" "$bout/.config" \
        "$ext/configs/kvmpanel-jpeg.fragment" "$ext/configs/kvmpanel-peripherals.fragment"
    make -C "$br" O="$bout" olddefconfig
    make -C "$br" O="$bout"
)

mkdir -p "$qout" "$work/build/plan2"
make -C "$work/upstream/linux" O="$qout" sunxi_defconfig
"$linux/scripts/kconfig/merge_config.sh" -m -O "$qout" "$qout/.config" \
    "$files/configs/kvmpanel.fragment" "$files/configs/qemu-virt.fragment" \
    "$files/configs/kvmpanel-gadget.fragment" "$files/configs/qemu-gadget.fragment"
make -C "$work/upstream/linux" O="$qout" olddefconfig
make -C "$work/upstream/linux" O="$qout" -j12 zImage
python3 "$files/qemu-smoke.py" "$qout/arch/arm/boot/zImage" "$bout/images/rootfs.cpio.gz" \
    --log "$work/build/plan2/qemu-smoke.log"
sh "$files/qemu-gadget-smoke.sh" "$qout/arch/arm/boot/zImage" "$bout/images/rootfs.cpio.gz" \
    "$work/build/plan2/qemu-gadget.log"
```

QEMU 测试内核关闭 HIGHMEM，使 usb-storage 可在无 DMA 的 dummy_hcd 上读取 LUN。板级内核保留自身内存配置。板上枚举使用 `kvmpanel-gadget start`；选择功能前用 `kvmpanel-gadget budget` 查看端点，修改组合需先 `stop`。具体开关和镜像参数见 `kvmpanel-gadget --help`。

在组合源码树复测模块时，QEMU 使用模块配置作为起点，追加相同的两个 virt/gadget 测试片段。模块安装到独立目录，和目标架构工具一起进测试 initramfs。

```bash
mq="$work/build/linux-plan2-qemu"
mkdir -p "$mq"
cp "$mout/.config" "$mq/.config"
"$linux/scripts/kconfig/merge_config.sh" -m -O "$mq" "$mq/.config" \
    "$files/configs/qemu-virt.fragment" "$files/configs/qemu-gadget.fragment"
make -C "$linux" O="$mq" olddefconfig
make -C "$linux" O="$mq" -j12 zImage modules
make -C "$linux" O="$mq" INSTALL_MOD_PATH="$mq/modules-root" modules_install
# 将 modules_install 产生的 lib/modules 追加至现有 initramfs。
(cd "$mq/modules-root" && find lib \( -type d -o -type f \) | cpio -o --quiet -H newc --owner 0:0) \
    > "$mq/modules.cpio"
cat "$bout/images/rootfs.cpio.gz" > "$mq/rootfs-modules.cpio.gz"
gzip -c "$mq/modules.cpio" >> "$mq/rootfs-modules.cpio.gz"
qemu-system-arm -machine virt,highmem=off -cpu cortex-a7 -smp 2 -m 512M \
    -nodefaults -nographic -serial mon:stdio -no-reboot -kernel "$mq/arch/arm/boot/zImage" \
    -initrd "$mq/rootfs-modules.cpio.gz" -append 'console=ttyAMA0 rdinit=/init'
```

来宾以 root 登录后执行 `modprobe lt8619c && modprobe sun20i-csi && modprobe panel-td4322`，用 `lsmod` 核对，再依次 `rmmod panel_td4322`、`rmmod sun20i_csi`、`rmmod lt8619c`。本次也在该来宾运行了 `cjpeg`、`djpeg`、`v4l2-ctl --version`、`gpiodetect --version` 和 `i2cdetect -V`。QEMU 的 virt 机器没有 T113-i 的显示或采集硬件。
