# Debian 13 SD 镜像

KVMPanel 的 SD 系统采用 Debian 13（trixie）armhf 的 `minbase`。镜像预装 Vim、systemd、APT、网口和音视频诊断工具，并运行现有 `kvmd` 和 USB gadget。板级内核、设备树、STC8 及 T113 远端核固件仍按项目源码构建。 [Debian ARMv7](https://www.debian.org/releases/trixie/armhf/) · [板级驱动](../../../docs/linux-6.18.md)

## 从 SD 启动

1. 使用 `kvmpanel-debian13-armhf.img`，若拿到的是 `.img.xz` 就先解压。选一张至少 4 GB 的 microSD，将镜像写入整张卡。写入会覆盖分区表和卡上数据。
2. 将卡插入 CARD1，接 USB2 串口看 UART0，参数为 115200 8N1。默认启动项为 3.3 V 的 SD High Speed。UHS 是独立菜单项，先在实板验证 PF 电平、调谐及读写，再切换。
3. 第一次启动按串口提示设置 root 密码，然后用 `root` 登录。用 `vim --version`、`findmnt /data`、`ip -4 addr show eth0` 和 `systemctl status kvmd` 检查系统。网口默认通过 DHCP 获取地址，`kvmd` 按既有流程初始化账号。

镜像大小为 2 GiB。MBR 后的第 16 扇区放 Allwinner SPL/U-Boot，第一分区是包含 `/boot/extlinux/extlinux.conf` 的 ext4 根分区，第二分区挂到 `/data`。KVM 持久配置位于 `/data/kvmd`，SD 文件通过 `/mnt/sd` 访问 `/data/sd`。更换已有镜像时先备份第二分区。需要存放大于 508 MiB 的介质文件时，写卡后可将第二分区及其 ext4 扩至卡的剩余空间。 [U-Boot Allwinner 启动](https://docs.u-boot.org/en/latest/board/allwinner/sunxi.html) · [extlinux](https://docs.u-boot.org/en/latest/develop/bootstd/extlinux.html)

### KVM 与断电

`systemctl status kvmd` 查看服务。其启动入口沿用应用升级和回滚逻辑。网页上的 DHCP、静态 IPv4 和 DNS 设置会更新 systemd-networkd 的 `20-eth0.network`。Debian 的 `gpiod` 2.x 可用于交互操作，`kvmd` 的服务路径优先查找私有的 1.x `gpioget` 和 `gpioset`，支持本项目既有的 GPIO 命令。SSH 和 mDNS 由 `kvmd` 根据保存的设置开关。 [networkctl](https://manpages.debian.org/trixie/systemd/networkctl.1.en.html)

正常关机执行 `systemctl --check-inhibitors=yes poweroff`。systemd 停止服务并清理存储后，STC8 helper 检查可写挂载和 swap，再请求 STC8 进入待断电状态，最后由内核经 UART2 发断电帧。重启不走这个路径。在线升级沿用 `--hold-wired` 并持有 systemd 的关机阻止锁，使用条件见 [STC8 说明](../../firmware/stc8/README.md#在线升级)。 [systemd-shutdown](https://www.freedesktop.org/software/systemd/man/latest/systemd-shutdown.html)

遇到 SD 启动问题，可按 [FEL 恢复](../u-boot/README.md#加载到内存)先验证 DDR 和内核。若 U-Boot 和 Linux 已可通过 FEL 运行，可加载默认 High Speed DTB，指定 `root=PARTUUID=<卡上第一分区的 PARTUUID> rootwait ro` 来验证 Debian 根分区。

## 构建

在 Linux 或 Ubuntu WSL 构建。需要 `mmdebstrap`、`sfdisk`、`e2fsprogs`、`dtc` 和 Debian 13 归档密钥。x86 主机需要能通过 binfmt 和 QEMU 执行 armhf，先用 `arch-test armhf` 确认。Ubuntu 如果只有旧版 Debian keyring，可将 Debian 官方的 `debian-archive-keyring` 软件包解包后，将其中的 `usr/share/keyrings/debian-archive-keyring.gpg` 路径传给 `bootstrap.sh`。 [Debian 归档密钥](https://ftp-master.debian.org/keys.html) · [密钥软件包](https://packages.debian.org/trixie/debian-archive-keyring)

先按 [内核及组合 DTB](../../../docs/development.md#构建应用所需的内核模块和组合-dtb) 构建 `zImage` 和 `sun8i-t113i-kvmpanel-drivers.dtb`。构建 SD 用的 U-Boot 时使用已有的 FEL 配置并选择 SD 设备树。

```bash
set -e
project=/mnt/d/MyProj/KVMPanel
work="$HOME/work/KVMPanel"
ubsrc="$work/upstream/u-boot"
ubout="$work/build/u-boot-kvmpanel-sd"
stage="$ubsrc/dts/upstream/src/arm/allwinner"

for file in sun8i-t113i-kvmpanel.dts kvmpanel-stc-power.dtsi; do
    source="$project/software/os/linux/dts/$file"
    test -e "$stage/$file" || ln -s "$source" "$stage/$file"
    cmp "$source" "$stage/$file"
done
file=sun8i-t113i-kvmpanel-sd.dts
source="$project/software/os/u-boot/$file"
test -e "$stage/$file" || ln -s "$source" "$stage/$file"
cmp "$source" "$stage/$file"
mkdir -p "$ubout"
cp "$project/software/os/u-boot/configs/kvmpanel_fel_defconfig" "$ubout/.config"
"$ubsrc/scripts/config" --file "$ubout/.config" --set-str DEFAULT_DEVICE_TREE allwinner/sun8i-t113i-kvmpanel-sd
make -C "$ubsrc" O="$ubout" CROSS_COMPILE=arm-linux-gnueabihf- olddefconfig
make -C "$ubsrc" O="$ubout" CROSS_COMPILE=arm-linux-gnueabihf- -j"$(nproc)"
```

用 [现有应用交叉构建](../../../docs/development.md#构建-kvmd-与验证镜像) 准备 Buildroot 的 `target/` 目录，使用当前代码重新构建 `kvmd` 和 `kvmpanel-stc-power`。这里只取静态应用和 USB 脚本。若启用了 `BR2_PACKAGE_KVMPANEL_AMP`，也会带上 C906 和 HiFi4 的回环固件及调试程序。固件若由另一份 Buildroot 构建提供，可通过 `BUILDROOT_AMP_TARGET` 指定其 `target/`。网页按 [网页构建说明](../../app/web/README.md) 生成 `dist/`。下例可将 `kout` 和 `bout` 改为已有构建目录。使用 WSL 时，把输出目录放在 Linux 文件系统，以保留根文件系统的所有权和权限。

```bash
kout="$work/build/linux-plan2"
bout="$work/build/buildroot-plan2"
out="$work/build/debian-kvmpanel"
mkdir -p "$out"
sudo sh "$project/software/os/debian/bootstrap.sh" "$out/base-armhf.tar"
sudo bash "$project/software/os/debian/build-image.sh" "$out/base-armhf.tar" \
    "$ubout/u-boot-sunxi-with-spl.bin" "$kout" "$bout/target" \
    "$project/software/app/web/dist" "$out/kvmpanel-debian13-armhf.img"
```

构建脚本只生成镜像文件。Buildroot 的 FEL 诊断镜像另行按原流程构建。

## 离线验证

2026-11-23 构建了板用 U-Boot、内核和 Debian SD 镜像，检查了分区、两个 ext4 文件系统、SPL 偏移和两份启动 DTB。在 512 MiB 的 QEMU Cortex-A7 客体用快照验证了首次密码设置、Vim 9.1、数据分区挂载、DHCP、网页与 `/api/health`、USB gadget 配置，以及通过 KVM API 改静态地址和 DNS。虚拟串口测试证明可写根分区时只发取消命令，systemd 清理存储后依次发送准备和待断电命令。关机阻止锁也通过测试。SD 冷启动、最后的 UART2 断电帧和实板外设请见 [当前交接](../../../HANDOFF.md)。

### QEMU 内存占用

2026-11-23 测量同一份 Debian 13 SD 镜像。QEMU 配置为双核 Cortex-A7、512 MiB RAM 和 32 MiB CMA，Linux 6.18.64 报告 `MemTotal` 为 492.2 MiB。进入用户态后每 200 ms 采样，系统占用按 `MemTotal - MemAvailable` 计算。启动窗口截止到串口登录且 `kvmd` 已启动约 3 秒后。

| 阶段 | 系统占用峰值 |
|---|---:|
| 首次启动，含 root 密码设置 | 63.0 MiB |
| 第二次启动 | 108.6 MiB |
| 第二次启动后空闲 30 秒 | 76.5 MiB |

空闲 30 秒占用 69.6～76.5 MiB，平均 70.7 MiB，窗口结束时可用 422.6 MiB。`kvmd` 首次与第二次启动的采样时点峰值 RSS（`VmHWM`）分别为 6.7 和 36.2 MiB，空闲窗口结束时的 PSS 为 36.2 MiB。采样进程的 RSS 约 0.4 MiB，计入系统占用。测量时 Vim 未启动，无活跃 KVM 会话。实板 HDMI 采集、编码、显示和 DMA 缓冲的峰值待测。

旧版用户态已用相同测试内核、采样方式和已有 `/data` 复测。 [Buildroot 内存对照](../buildroot/README.md#qemu-内存占用)
