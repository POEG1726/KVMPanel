# FEL 启动配置

SD 镜像使用带 SMHC0 节点的 DTB，构建和写卡见 [Debian SD 镜像](../debian/README.md)。

`configs/kvmpanel_fel_defconfig` 用于 U-Boot 2026.10。SD 和 FEL 均使用这份配置，在 SPL 启用 T113 DDR PHY 读端 ODT，并用 `MR1=0x40` 设置 DDR3 颗粒的 40 Ω 输出和 120 Ω RTT_NOM。SD 启动的 SPL 和 U-Boot 控制 DT 都将 SDC0 的 PF0–PF5 设为 Level 3，典型 45 Ω。 [T113-i 启动链与 DDR 参数](../vendor/tina/README.md)

## 构建

U-Boot 将同一份板级 DTS 与自身的 sunxi DTSI 一起编译，以生成 U-Boot 控制 DT 和启动镜像。使用 WSL 中的源码副本和 [编译依赖](../../../docs/development.md)。

```sh
project=/mnt/d/MyProj/KVMPanel
work="$HOME/work/KVMPanel"
ubsrc="$work/upstream/u-boot"
ubout="$work/build/u-boot-kvmpanel-fel"
dts="$project/software/os/linux/dts"
staged_dts="$ubsrc/dts/upstream/src/arm/allwinner"
patch="$project/software/os/u-boot/patches/0001-sunxi-configure-sdc0-drive-and-dram-mr1.patch"

git -C "$ubsrc" apply --reverse --check "$patch" 2>/dev/null ||
  git -C "$ubsrc" apply "$patch" || exit
mkdir -p "$ubout"
for file in sun8i-t113i-kvmpanel.dts kvmpanel-stc-power.dtsi; do
  test -e "$staged_dts/$file" || ln -s "$dts/$file" "$staged_dts/$file"
  cmp "$dts/$file" "$staged_dts/$file" || exit
done
cp "$project/software/os/u-boot/configs/kvmpanel_fel_defconfig" "$ubout/.config" &&
  make -C "$ubsrc" O="$ubout" CROSS_COMPILE=arm-linux-gnueabihf- olddefconfig &&
  make -C "$ubsrc" O="$ubout" CROSS_COMPILE=arm-linux-gnueabihf- -j"$(nproc)"
```

## 加载到内存

以下是待上板验证的步骤。USB1 是直连 SoC 的 FEL 口，USB2 是 CH343K 调试串口，两口的供电关系见[板级接线](../../../hardware/README.md)。Linux 主机可通过 `sunxi-tools` 提供的 `sunxi-fel ver` 确认设备。首次可以只传 U-Boot，在串口查看 SPL 的 DDR 输出并执行 `bdinfo`。确认内存范围后，再用 FEL 加载三件 Linux 产物。

```sh
kout="$work/build/linux-kvmpanel"
bout="$work/build/buildroot-kvmpanel"
initramfs="$bout/images/rootfs.cpio.gz"

sunxi-fel -p uboot "$ubout/u-boot-sunxi-with-spl.bin" \
  write 0x42000000 "$kout/arch/arm/boot/zImage" \
  write 0x43000000 "$kout/sun8i-t113i-kvmpanel.dtb" \
  write 0x43400000 "$initramfs"
printf 'setenv rdsize 0x%x\n' "$(stat -c %s "$initramfs")"
```

在串口 U-Boot 提示符粘贴上面输出的 `setenv rdsize` 命令，然后执行：

```text
setenv bootargs 'console=ttyS0,115200 earlycon rdinit=/init'
bootz 0x42000000 0x43400000:${rdsize} 0x43000000
```

内核到达 shell 后，可查看 `/proc/meminfo`、`/sys/devices/system/cpu/online` 和启动日志。FEL 只把这些镜像传到 RAM。本板预留 microSD 持久启动，镜像布局待实板验证。
