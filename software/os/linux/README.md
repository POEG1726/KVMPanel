# Linux 适配

当前用 Linux 6.18.64。完整源码的位置见根目录 `sources/README.md`。

组合设备树包含 SMHC0 的 PF 电压切换与 UHS-I、SGM3851A 默认上电和 SWIRE 电压接口，以及带 PC3 复位和 PE17 中断的 LT8619C。基础设备树也启用使用无源晶体的 RTL8211F-CG。构建前按编号应用补丁。 [适配说明](../../../docs/linux-6.18.md)

- `dts/sun8i-t113i-kvmpanel.dts`：板级设备树，含 UART、DDR 容量、USB 和以太网配置。组合 DTS 的外设片段描述 microSD 卡座。
- `configs/kvmpanel.fragment`：叠加到 `sunxi_defconfig`，启用串口、initramfs、USB 存储和通用 SPI/MTD 驱动。
- `configs/qemu-virt.fragment` 和 `qemu-smoke.py`：在独立构建目录启用 QEMU `virt` 并验证 initramfs。
- `patches/`：按应用顺序编号的补丁，记录 upstream 基线、来源和验证状态。
- `dts/kvmpanel-stc-power.dtsi` 和补丁 0008：PB0/PB1 的 UART2、GPADC0 和 STC8 最终断电通知。正常关机同时使用 Buildroot 的配套 BusyBox 与串口工具。 [STC8 开发说明](../../firmware/stc8/README.md)
- `dts/kvmpanel-peripherals.dtsi` 和 `configs/kvmpanel-peripherals.fragment` 启用 PG13 上四颗 WS2812B 的硬件 LEDC 和 Linux 多色 LED 接口。 [控制方式与验证](../../../docs/linux-6.18.md#ws2812)

构建见 [开发说明](../../../docs/development.md)。首次上板先通过 FEL 验证 DDR 和串口，详见[板级接线](../../../hardware/README.md)。
