# T113-i 的 DDR 和启动介质

## 外置 DDR3

本板的 `K4B4G1646D-BCK0` 是三星 1.5 V DDR3，256M×16，单颗为 512 MiB。DTS 声明从 `0x40000000` 开始有 512 MiB。DDR 控制器和 PHY 由 Boot0/SPL 在 Linux 启动前初始化。U22 的 CS# 按设计固定接地，当前 FEL 版 SPL 会自动探测 rank 和位宽。首次上板先验证训练和探测容量，再做内存读写测试。 [板级接线](../../../../hardware/README.md) · [三星数据手册](https://download.semiconductor.samsung.com/resources/data-sheet/DS_K4B4G1646D-BC-I_Rev103-0.pdf)

Tina 参考配置是 `tina-t113i-overlay/device/config/chips/t113_i/configs/evb1_auto_nand/sys_config.fex` 的 `[dram_para]`。它使用 DDR3、792 MHz、`ZQ=0x7b7bfb`、`ODT=0x1`、`TPR11=0x00770000`、`TPR12=0x2`、`TPR13=0x34050100`。从百问网 T113i DevKit 默认固件提取的 `boot0_nand.fex`、`boot0_sdcard.fex` 和 `fes1.fex` 通过 eGON 校验和检查，且在偏移 `0x38` 内嵌与该配置一致的 24 个 DDR 参数字。参考件存放在 `sources/donors/tina-t113i-devkit-default/`。 [板级 overlay](https://github.com/DongshanPI/T113i_DevKitF_Tina5SDK) · [参考固件](https://dshanpi.100ask.net/en/docs/T113i-Industrial/part1/03-1_FlashSystem/)

KVMPanel 的 SD 和 FEL SPL 将三星颗粒的 `MR1` 设为 `0x40`，其中 A5/A1 为 `00`（40 Ω 输出），A9/A6/A2 为 `010`（RTT_NOM 120 Ω）。2026-11-22 网表的 `DDR_ODT` 连通 T113 的 SODT0 与颗粒的 ODT。主控读端 ODT 已由 `CONFIG_DRAM_SUNXI_ODT_EN=0x1` 启用。T113 DDR PHY 使用参考 `ZQ=0x7b7bfb`，待取得该 PHY 的 ZQ 字段定义和阻抗表，再选择 48 Ω、40 Ω 或最强档。 [三星 DDR3 MR1 定义](https://download.semiconductor.samsung.com/resources/device-operation-timing-diagram/ddr3_device_operation_timing_diagram_rev14-2.pdf) · [板级参数](../../u-boot/configs/kvmpanel_fel_defconfig) · [2026-11-22 网表](../../../../hardware/exports/2026-11-22/Netlist_Schematic2_2026-11-22.enet)

### 主控 ZQ 编码查证

分别反汇编了百问网 DevKit 的 SD Boot0、NAND Boot0 和 FES1，以及 Tina SPL 镜像的 `board/sun8iw20p1/libsun8iw20p1_sdcard.a`。三份固件的 `dram_zq` 都在文件偏移 `0x40`，值为 `0x007b7bfb`。在 SD Boot0 偏移 `0x3654–0x366e`，代码从参数块读 `dram_zq`，把低 24 位直接写到 `0x03103140`，同时置 bit 25。Tina `mctl_channel_init` 和主线 SPL 执行同样的操作，阻抗由 PHY 解释。Tina 的配置指南将该参数交由原厂调节，未给出字段和档位表。 [Tina SPL 镜像](https://gitee.com/xiaobocwb/tina-spl-pub/tree/f436101c3dad52769e8e338bc207941a0e0df3b0) · [Tina 配置指南 §3.3](https://tina.100ask.net/SdkModule/Linux_Deploy_DevelopmentGuide-02/) · [主线 SPL](https://github.com/u-boot/u-boot/blob/v2026.10/drivers/ram/sunxi/dram_sun20i_d1.c#L793)

| 已见编码 | 核对到的用途 |
|---|---|
| `0x7b7bfb` | T113-i DevKit、T113 EVB/MangoPi MQ-R、T113 Pro 和 D1 MQ-Pro 的 DDR3。Tina 的 R528 SPL 库也以它作为 `mctl_init` 的 DDR3 默认值。 [T113/MQ-R](https://github.com/paulwratt/mangopi-sbc-Tina-Linux/blob/352fc38e6dfbb6f5d7b7be06201c54b7469885e9/device/config/chips/t113/configs/mq_r/sys_config.fex#L165) · [T113 Pro](https://github.com/DongshanPI/100ASK_T113-Pro_TinaSDK/blob/master/device/config/chips/t113/configs/100ask/sys_config.fex#L143) |
| `0x7b7bf9` | F133 EVB/MQ-R 的 DDR2，XFEL 的 D1 和 F133 `mctl_hal.S` 逐字节相同，DDR 参数块分别传入 `0x7b7bfb` 和 `0x7b7bf9`。 [F133/MQ-R](https://github.com/paulwratt/mangopi-sbc-Tina-Linux/blob/352fc38e6dfbb6f5d7b7be06201c54b7469885e9/device/config/chips/f133/configs/mq_r/sys_config.fex#L152) · [XFEL 参数](https://github.com/xboot/xfel/blob/445e8aefe6914c85817cc9bd1d201629364b0ec6/chips/d1_f133.c#L2938) |
| `0x3b3bf9` / `0x3b3bfb` | XFEL 的 A40i/R40 参数，分别用于该芯片的 DDR3 和 LPDDR3。 [XFEL A40i/R40 参数](https://github.com/xboot/xfel/blob/445e8aefe6914c85817cc9bd1d201629364b0ec6/chips/a40i_r40.c#L1442) |

STM32MP1 的 ZPROG 低半字节 `0x9` 对应 48 Ω，`0xb` 对应 40 Ω。RK3188 的 ZPROG 也将驱动与 ODT 分配到低、高半字节。这给出了一个待验证的解释。若 T113 的三个字节采用相同的分配和换算，`0x7b7bfb` 的三个驱动子字段是 `b/b/b`，把它们都换成 `9` 得到 `0x7979f9`。T113 的位域、各字节对应的 I/O 和 Ω 换算尚无同型号证据，因此这个推算未用于 SPL。接手需要 `0x03103140[23:0]` 的字段表及 240 Ω 校准参考下的输出阻抗表，或在实板上独立确认阻抗。 [ST AN5168 表 9](https://www.st.com/resource/en/application_note/an5168-ddr-configuration-on-stm32mp1-series-mpus-stmicroelectronics.pdf#page=17) · [RK3188 TRM 的 DDR_PHYCTL_ZQ0CR1](https://rockchip.fr/Rockchip%20RK3188%20TRM%20V1.3.pdf)

调研源码在 WSL 的 `~/work/KVMPanel/upstream/` 下，目录为 `tina-spl-pub-mirror`、`tina-mangopi-mirror` 和 `xfel`。反汇编中间文件在 `~/work/KVMPanel/build/t113-zq-research/`。

## 先从 RAM 加载 Linux

本项目使用主线 U-Boot 的 R528 DDR3 实现及上述板级值，用 USB FEL 在 RAM 中运行 SPL 和 U-Boot，再传入 `zImage`、本板 DTB、Buildroot `rootfs.cpio.gz`。网表确认 USB1 直连 SoC USB0，USB2 经 CH343K 接 UART0 PB8/PB9。上板后检查 SPL 训练、内存容量、内存读写和内核的 CPU 上线数。 [FEL 启动配置](../../u-boot/README.md)

## microSD 冷启动

2026-11-22 版以 CARD1 microSD 为板载启动介质。验证顺序是 FEL 加载到 RAM、核对 SMHC0 的只读访问，再验证从 SD 装载 SPL、U-Boot 和 Debian。镜像使用 MBR 和两个 ext4 分区，冷启动待实板确认。 [Debian 镜像](../../debian/README.md) · [板级接线](../../../../hardware/README.md)

## 旧版的 SPI NAND 调研

本版已移除板载 SPI NAND。下列记录保留 Tina 参考启动链与驱动调研结果。

- **Tina 的 NAND 读取。** 参考链路是 `BROM → boot0_nand → boot_package → U-Boot → boot`。NAND 方案的 `storage_type=5`。打包程序用本板 `sys_config.bin` 更新 Boot0，U-Boot 包含在 `boot_package.fex`。参考启动环境先用 `sunxi_flash read 43000000 boot` 读逻辑 `boot` 分区，再用 `bootm 43000000` 加载 Android 格式的 `boot.img`。参考固件归档里同时有 SD 与 NAND Boot0，包内 `sys_config.fex` 的 `storage_type=1`，上面的 NAND 环境来自 `evb1_auto_nand` 源码。 [Tina 打包框架](https://github.com/100askTeam/Tina5-LinuxSDK) · [NAND 参考板配置](https://github.com/DongshanPI/T113i_DevKitF_Tina5SDK/tree/master/device/config/chips/t113_i/configs/evb1_auto_nand)
- **W25N04KWZEIR。** Linux 6.18.64 和主线 U-Boot 2026.10 的 SPI NAND 设备表均匹配 `EF BA 23`，几何为 2048 B 数据页＋128 B OOB、64 页/擦除块、4096 块，ECC 为 8 bit/512 B。参考 `boot0_nand.fex` 和 Tina U-Boot 中未找到这个 ID 的连续字节，参考 U-Boot 中可见 W25N01GV 与 W25N02KV。主线 U-Boot 的 SPI NAND 设备表只证明 U-Boot proper 有对应驱动。Sunxi SPL 的专用 SPI 加载路径读取 NOR，无法据此确认 NAND 冷启动。
