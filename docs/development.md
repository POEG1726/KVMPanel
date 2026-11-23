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

QEMU 使用 `virt` 和两个 Cortex-A7，加载同一份 Buildroot initramfs。测试脚本通过串口登录，检查两核、通过 DHCP 获取虚拟网段地址、探测网关、读取只读 USB 磁盘，启动模拟模式的 kvmd，并在来宾用 curl 请求 /api/health，最后关机。虚拟网络启用了 `restrict=on`。

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

QEMU 的串口、网卡和 USB 控制器使用 PL011、VirtIO 和 xHCI。T113-i 的 DDR 训练、FEL、RTL8211F、MUSB/EHCI/OHCI 和 SMHC0 需要在实板验证。

## 构建 kvmd 与验证镜像

`kvmpanel_defconfig` 启用本地包 `package/kvmd` 并安装 curl。WSL 用户目录需安装 Rust stable 和目标标准库。包使用应用的 `Cargo.lock`，从对应的 `software/app/api/` 读取契约。C 依赖由 Buildroot 的 Bootlin 工具链编译，最终静态链接目标 sysroot 中的 musl 和 libjpeg-turbo。

```bash
project=/mnt/d/MyProj/KVMPanel
work="$HOME/work/KVMPanel"
br="$work/upstream/buildroot"
bout="$work/build/buildroot-plan2"
qout="$work/build/linux-kvmpanel-qemu"
"$HOME/.cargo/bin/rustup" target add --toolchain stable armv7-unknown-linux-musleabihf
(
    unset ARCH CROSS_COMPILE LD_LIBRARY_PATH
    export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
    make -C "$br" O="$bout" BR2_EXTERNAL="$project/software/os/buildroot" kvmpanel_defconfig
    make -C "$bout" -j12
)
mkdir -p "$work/logs"
python3 "$project/software/os/linux/qemu-smoke.py" \
    "$qout/arch/arm/boot/zImage" "$bout/images/rootfs.cpio.gz" \
    --log "$work/logs/kvmd-qemu.log"

# 使用下面“组合验证”里的 dummy_hcd/ACM 内核复测同一镜像。
sh "$project/software/os/linux/qemu-gadget-smoke.sh" \
    "$work/build/linux-plan2-qemu/arch/arm/boot/zImage" "$bout/images/rootfs.cpio.gz" \
    "$work/logs/kvmd-qemu-gadget.log"
```

更换了应用源码或契约后，在上述子 shell 内执行 `make -C "$bout" kvmd-reconfigure`，然后 `make -C "$bout"` 重新生成镜像。包默认使用 `~/.cargo/bin/cargo` 和同目录下的 rustc，其他安装位置可通过 make 的 `KVMD_CARGO`、`KVMD_RUSTC` 变量指定。这一版 Rust 目标标准库使用自带的 libunwind；包仅取其静态 unwind 库，其余启动文件和 C 库来自 Bootlin。

板上由 `/etc/init.d/S95kvmd` 启动服务。将持久分区挂到 `/data` 后即可启动；若开机时挂载还未完成，完成后再执行 `/etc/init.d/S95kvmd start`。升级后的程序从 `/data/kvmd/apps/current/kvmd` 加载，随包网页位于同目录的 `web/`。基础镜像中的程序位于 `/usr/sbin/kvmd`，网页目录为 `/usr/share/kvmd/web`。板级路径和 GPIO 参数可在 `/etc/default/kvmd` 设置。

Dropbear 和 mdnsd 的启动脚本位于 `/usr/libexec/kvmd/`，由 kvmd 读取设置后调用；SSH 主机私钥保存在 `/data/kvmd/ssh/`。

QEMU 冒烟脚本在临时目录启动 `kvmd --sim`，从来宾访问 `http://127.0.0.1:18800/api/health` 并记录实际 JSON。共享盘需要内核 loop 驱动；Tailscale 需要 TUN、IPv4/IPv6 策略路由和 nftables。基础内核片段已启用这些选项，脚本会在来宾挂载一个临时 loop 镜像、验证本地只读切换和恢复写入，检查 TUN 设备和两套路由规则。

2026-11-20 的镜像在双核 Cortex-A7 来宾返回 `{"ok":true,"version":"0.1.0"}`。包内 kvmd 是 ARMv7 EABI5 静态 ELF，使用 VFP 寄存器传参。相同镜像在 dummy_hcd 下通过三 LUN 读回、EP0 遥测、相对或绝对鼠标共享遥测、端点超预算拒绝和 ACM 枚举测试。

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
4. 网口和 microSD 分开验证。网口用 `ip link set eth0 up`、`ethtool eth0` 检查链路，再用 `udhcpc -n -q -i eth0` 获取地址并测试收发。CARD1 用组合 DTB 验证。先测 PF 识别时的 3.3 V 和 UHS 协商后的 1.8 V，再用 `dmesg` 和 `/sys/kernel/debug/mmc0/ios` 核对模式、实际频率、容量与只读读取。

基础外设跑通后，用 [Debian SD 镜像](../software/os/debian/README.md) 验证 microSD 冷启动和持久存储。镜像把 SPL/U-Boot 放在卡头，内核和 DTB 由 extlinux 从第一分区读取。首次使用默认 High Speed，FEL 和 Buildroot initramfs 可用于恢复与诊断。

## 单独验证本板网口

当前软件按用户确认的 RTL8211F-CG 无源 25 MHz 晶体方案配置。晶体跨接 U17.36 `XTAL_IN` 和 U17.37 `XTAL_OUT`。2026-11-22 归档仍是 PG11 经 R235 驱动 `EXT_CLK`、`XTAL_IN` 接地的旧接法，新接法待核对新版导出。`3V3_MAIN` 为 PHY 主电源及 RGMII I/O，芯片开关稳压器通过 L100 产生 `PHY_1V0`。 [旧版网表](../hardware/exports/2026-11-22/Netlist_Schematic2_2026-11-22.enet) · [无源晶体参考原理图](https://dl.100ask.net/Hardware/MPU/T113i-Industrial/T113-i_Industrial-DevKit_V12_PLUS.pdf#page=4)

- **开机和复位**。按 Rev. 1.7 核对 `3V3_MAIN` 的上升时间，常规范围为 0.5–100 ms。PC1 拉低 `PHYRSTB` 至少 10 ms，释放后留 200 ms 给 `PHY_1V0` 上升、72 ms 内部稳定时间和 Note 6 所述的保留窗口，再访问地址 1 的 MDIO。 [Realtek Rev. 1.7 §7.17、§9.3](https://www.ic-golden.com/wp-content/uploads/2024/08/rtl8211fi-cg-datasheet.pdf)
- **关机和重上电**。连续断电重开时，确认 `3V3_MAIN` 和 `PHY_1V0` 都降到 0 V，断电间隔超过 100 ms。首次上板核对上电、重启和断电波形，以及 PHY 起振、MDIO 和 PC7 低电平中断。 [Realtek Rev. 1.7 §9.3](https://www.ic-golden.com/wp-content/uploads/2024/08/rtl8211fi-cg-datasheet.pdf)
- **软件配置**。EMAC 已启用，设备树使用 PC1 低有效复位、10 ms 拉低和 200 ms 释放后等待，以及 PC7 低电平中断。PHY 节点由晶体提供参考时钟，PG11 已从 EMAC pinmux 去除。`rgmii-id` 待结合实际走线和千兆链路验证。 [板级 DTS](../software/os/linux/dts/sun8i-t113i-kvmpanel.dts)

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
for patch in "$files"/patches/000{1,2,3,4,5,6,7,8}-*.patch; do
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

## C906 和 HiFi4

组合 DTB 注册 C906、HiFi4 和它们的 mailbox。默认在 root shell 手动启动各核，固件由 Linux 从 `/lib/firmware/kvmpanel/` 加载。固件只提供 RPMsg 回环服务。Buildroot 选择 `BR2_PACKAGE_KVMPANEL_AMP=y` 后一起编译并安装两份 ELF 和 `kvmpanel-amp-ping`。普通 Buildroot 配置可以继续单独生成基础系统镜像。

在 WSL 准备 RISC‑V bare-metal GCC 和 HiFi4 GCC 10.3，把实际编译器前缀传给 Buildroot。本次 RISC‑V 用 Ubuntu 24.04 的 `gcc-riscv64-unknown-elf` 和 `binutils-riscv64-unknown-elf`。 [HiFi4 GCC 10.3](https://github.com/YuzukiHD/FreeRTOS-HIFI4-DSP/releases/tag/Toolchains)

```bash
work="$HOME/work/KVMPanel"
project=/mnt/d/MyProj/KVMPanel
br="$work/upstream/buildroot"
ext="$project/software/os/buildroot"
bout="$work/build/buildroot-plan2"
export PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin
export RISCV_CROSS_COMPILE="$work/upstream/riscv64-elf-cross/usr/bin/riscv64-unknown-elf-"
export XTENSA_CROSS_COMPILE="$work/upstream/xtensa-hifi4-cross/bin/xtensa-hifi4-elf-"
make -C "$br" O="$bout" BR2_EXTERNAL="$ext" menuconfig
# 选 KVMPanel packages 下的 C906 and HiFi4 echo firmware，然后保存。
make -C "$br" O="$bout" BR2_EXTERNAL="$ext" -j12
```

本次构建的 `zImage`、`sun8i-t113i-kvmpanel-drivers.dtb` 和 `rootfs.cpio.gz` 放在 WSL 的 `~/work/KVMPanel/build/t113-amp/`。按 FEL 流程加载这三个文件。第一次在串口上分别启动两个核，确认各自的 RPMsg 设备名。 [FEL 加载方法](../software/os/u-boot/README.md#加载到内存)

```sh
for proc in /sys/class/remoteproc/remoteproc*; do
    printf '%s ' "$proc"
    cat "$proc/name" "$proc/firmware" "$proc/state"
done

# 替换为上面对应核的实际路径。
proc=/sys/class/remoteproc/remoteproc0
echo start > "$proc/state"
for dev in /sys/class/rpmsg/rpmsg[0-9]*; do
    [ -e "$dev" ] && readlink -f "$dev"
done
# 从路径中核对所属 remoteproc，替换成实际节点。
kvmpanel-amp-ping /dev/rpmsg0
echo stop > "$proc/state"
```

再同时启动两核，分别回环并重复停止和启动，检查串口是否有 mailbox 或 remoteproc 错误。回环固件轮询 mailbox 和共享队列，测试结束后可手动停止。SD 启动使用同一套 DT 节点和固件路径。

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
