# T113-i 的 DDR 和启动介质

## 外置 DDR3

本板的 `K4B4G1646D-BCK0` 是三星 1.5 V DDR3，256M×16，单颗为 512 MiB。DTS 声明从 `0x40000000` 开始有 512 MiB。DDR 控制器和 PHY 由 Boot0/SPL 在 Linux 启动前初始化。U22 的 CS# 按设计固定接地，当前 FEL 版 SPL 会自动探测 rank 和位宽。首次上板先验证训练和探测容量，再做内存读写测试。 [板级接线](../../../../hardware/README.md) · [三星数据手册](https://download.semiconductor.samsung.com/resources/data-sheet/DS_K4B4G1646D-BC-I_Rev103-0.pdf)

Tina 参考配置是 `tina-t113i-overlay/device/config/chips/t113_i/configs/evb1_auto_nand/sys_config.fex` 的 `[dram_para]`。它使用 DDR3、792 MHz、`ZQ=0x7b7bfb`、`ODT=0x1`、`TPR11=0x00770000`、`TPR12=0x2`、`TPR13=0x34050100`。从百问网 T113i DevKit 默认固件提取的 `boot0_nand.fex`、`boot0_sdcard.fex` 和 `fes1.fex` 通过 eGON 校验和检查，且在偏移 `0x38` 内嵌与该配置一致的 24 个 DDR 参数字。参考件存放在 `sources/donors/tina-t113i-devkit-default/`。目前取得了参考固件的启动组件和配置。对应的 Tina5 基础包含 `brandy/brandy-2.0/spl-pub`，还未取得。 [板级 overlay](https://github.com/DongshanPI/T113i_DevKitF_Tina5SDK) · [参考固件](https://dshanpi.100ask.net/en/docs/T113i-Industrial/part1/03-1_FlashSystem/) · [Tina5 基础包说明](https://dshanpi.100ask.org/docs/T113i-Industrial/part3/DevelopmentEnvironmentSetup/)

## 先从 RAM 加载 Linux

本项目用主线 U-Boot 的 R528 DDR3 实现承接同一组参考值，用 USB FEL 在 RAM 中运行 SPL 和 U-Boot，再传入 `zImage`、本板 DTB、Buildroot `rootfs.cpio.gz`。网表确认 USB1 直连 SoC USB0，USB2 经 CH343K 接 UART0 PB8/PB9。上板后检查 SPL 报告的内存容量和内核的 CPU 上线数。 [FEL 启动配置](../../u-boot/README.md)

## microSD 冷启动

2026-11-22 版以 CARD1 microSD 为板载启动介质。验证顺序是 FEL 加载到 RAM、核对 SMHC0 的只读访问，再验证从 SD 装载 SPL 和 U-Boot。镜像布局待上板确认。 [板级接线](../../../../hardware/README.md)

## 旧版的 SPI NAND 调研

本版已移除板载 SPI NAND。下列记录保留 Tina 参考启动链与驱动调研结果。

- **Tina 的 NAND 读取。** 参考链路是 `BROM → boot0_nand → boot_package → U-Boot → boot`。NAND 方案的 `storage_type=5`。打包程序用本板 `sys_config.bin` 更新 Boot0，U-Boot 包含在 `boot_package.fex`。参考启动环境先用 `sunxi_flash read 43000000 boot` 读逻辑 `boot` 分区，再用 `bootm 43000000` 加载 Android 格式的 `boot.img`。参考固件归档里同时有 SD 与 NAND Boot0，包内 `sys_config.fex` 的 `storage_type=1`，上面的 NAND 环境来自 `evb1_auto_nand` 源码。 [Tina 打包框架](https://github.com/100askTeam/Tina5-LinuxSDK) · [NAND 参考板配置](https://github.com/DongshanPI/T113i_DevKitF_Tina5SDK/tree/master/device/config/chips/t113_i/configs/evb1_auto_nand)
- **W25N04KWZEIR。** Linux 6.18.64 和主线 U-Boot 2026.10 的 SPI NAND 设备表均匹配 `EF BA 23`，几何为 2048 B 数据页＋128 B OOB、64 页/擦除块、4096 块，ECC 为 8 bit/512 B。参考 `boot0_nand.fex` 和 Tina U-Boot 中未找到这个 ID 的连续字节，参考 U-Boot 中可见 W25N01GV 与 W25N02KV。主线 U-Boot 的 SPI NAND 设备表只证明 U-Boot proper 有对应驱动。Sunxi SPL 的专用 SPI 加载路径读取 NOR，无法据此确认 NAND 冷启动。
