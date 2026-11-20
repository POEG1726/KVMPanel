# FEL 启动配置

`configs/kvmpanel_fel_defconfig` 用于 U-Boot 2026.10。它使用 R528 的 SPL DDR3 驱动、Tina T113-i 的 DDR3 参考参数、UART0 和 RAM 环境。目标是通过 FEL 启动，等待本板验证。参数来源和 NAND 冷启动见 [T113-i 启动链](../vendor/tina/README.md)。

## 构建

U-Boot 将同一份板级 DTS 与自身的 sunxi DTSI 一起编译，以生成 U-Boot 控制 DT 和启动镜像。使用 WSL 中的源码副本和 [编译依赖](../../../docs/development.md)。

```sh
project=/mnt/d/MyProj/KVMPanel
work="$HOME/work/KVMPanel"
ubsrc="$work/upstream/u-boot"
ubout="$work/build/u-boot-kvmpanel-fel"
dts="$project/software/os/linux/dts/sun8i-t113i-kvmpanel.dts"
staged_dts="$ubsrc/dts/upstream/src/arm/allwinner/sun8i-t113i-kvmpanel.dts"

mkdir -p "$ubout"
test -e "$staged_dts" || ln -s "$dts" "$staged_dts"
cmp "$dts" "$staged_dts" &&
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

内核到达 shell 后，可查看 `/proc/meminfo`、`/sys/devices/system/cpu/online` 和启动日志。FEL 只把这些镜像传到 RAM。持久启动的 Flash 布局取决于本板上实际工作的 Boot0 和 U-Boot。
