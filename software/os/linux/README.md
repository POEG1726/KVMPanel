# Linux 适配

当前用 Linux 6.18.64。完整源码的位置见根目录 `sources/README.md`。

- `dts/sun8i-t113i-kvmpanel.dts`：板级设备树，含 UART、DDR 容量、SPI NAND、USB 和以太网配置。
- `configs/kvmpanel.fragment`：叠加到 `sunxi_defconfig`，启用串口、initramfs、SPI NAND 和 USB 存储。
- `configs/qemu-virt.fragment` 和 `qemu-smoke.py`：在独立构建目录启用 QEMU `virt` 并验证 initramfs。
- `patches/`：按应用顺序编号的补丁，记录 upstream 基线、来源和验证状态。

构建见 [开发说明](../../../docs/development.md)。首次上板先通过 FEL 验证 DDR 和串口，详见[板级接线](../../../hardware/README.md)。
