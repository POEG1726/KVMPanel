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

#### R528 与 D1 的板级配置和寄存器

- **R528。** Tina `board/r528/libsun8iw20p1_sdcard.a` 的 ARM 指令把 `dram_zq[23:0]` 写到 `0x03103140` 并置 bit 25，与 T113 相同。Centauri DDR3 固件的 Boot0 通过 eGON 校验和检查，参数为 792 MHz、DDR3、`0x7b7bfb`、主控读端 ODT 关闭。它在文件偏移 `0x33a4–0x33be` 有相同的 ZQCR 写法。R528-S2 封装内置 DDR2，N11 为 NC。R528-S3 内置 DDR3，N11 为 DZQ，外接 240 Ω ±1%。Tina `board/r528s2` 中的 FPGA 初始化入口仍带 DDR3 默认参数，不能用来确认 R528-S2 产品的 DDR2 板级编码。 [Tina R528 库](https://gitee.com/xiaobocwb/tina-spl-pub/tree/f436101c3dad52769e8e338bc207941a0e0df3b0/board/r528) · [Centauri Boot0](https://github.com/OpenCentauri/cc-fw-tools/blob/57049b3207d12b42512bb7c11734553c218958fe/uboot/boot0) · [R528 数据手册 V1.5](https://whycan.com/files/allwinner/r528/a/Hardware%E7%A1%AC%E4%BB%B6%E7%B1%BB%E6%96%87%E6%A1%A3/%E8%8A%AF%E7%89%87%E6%89%8B%E5%86%8C/R528_Datasheet_V1.5.pdf#page=13) · [R528 硬件设计 §3.4](https://whycan.com/files/allwinner/r528/a/Hardware%E7%A1%AC%E4%BB%B6%E7%B1%BB%E6%96%87%E6%A1%A3/%E7%A1%AC%E4%BB%B6%E8%AE%BE%E8%AE%A1/R528%E7%A1%AC%E4%BB%B6%E8%AE%BE%E8%AE%A1%E6%8C%87%E5%8D%97V1_1_20220325.pdf#page=17)
- **D1-H 和 D1s。** 哪吒和 `nezha_min` 的 `sys_config.fex` 均为 792 MHz DDR3、`0x7b7bfb`、主控读端 ODT 开启。D1s 哪吒是 528 MHz DDR2、`0x7b7bf9`、主控读端 ODT 关闭。RISC-V Tina 启动库的 `mctl_channel_init` 在 `0x1ee–0x214` 对相同寄存器执行相同写入，取低 24 位的指令为 `th.extu a5,a5,23,0`。库内 `mctl_init` 带 DDR2 默认值，正常 Boot0 调用 `init_DRAM` 并传入板级参数。 [哪吒](https://gitlab.com/weidongshan/tina-d1-h/-/blob/98e25a4f3817f5677658fa0e3fd86caea5223b44/device/config/chips/d1-h/configs/nezha/sys_config.fex#L136) · [`nezha_min`](https://gitlab.com/weidongshan/tina-d1-h/-/blob/98e25a4f3817f5677658fa0e3fd86caea5223b44/device/config/chips/d1-h/configs/nezha_min/sys_config.fex#L136) · [D1s 哪吒](https://gitlab.com/weidongshan/tina-d1-h/-/blob/98e25a4f3817f5677658fa0e3fd86caea5223b44/device/config/chips/d1s/configs/nezha/sys_config.fex#L153) · [Tina Boot0](https://gitee.com/xiaobocwb/tina-spl-pub/blob/f436101c3dad52769e8e338bc207941a0e0df3b0/nboot/main/boot0_main.c#L66)
- **厂商手册。** R528 V1.8 §3.5.3 和 D1-H V1.0 §3.4.3 定义了 System Control 模块的 `RESCAL_CTRL_REG`、`RES240_CTRL_REG` 和 `RESCAL_STATUS_REG`。它们位于 `0x03000160`、`0x03000168` 和 `0x0300016c`，用于 240 Ω 参考电阻的修调和状态读回。两份手册的 DRAMC 节止于功能概述，未公开 `0x03103140` 三个字节的引脚映射和输出 Ω 表。 [R528 V1.8](https://whycan.com/files/allwinner/r528/a/Hardware%E7%A1%AC%E4%BB%B6%E7%B1%BB%E6%96%87%E6%A1%A3/%E8%8A%AF%E7%89%87%E6%89%8B%E5%86%8C/R528_User_Manual_V1.8.pdf#page=185) · [D1-H V1.0](https://mangopi.org/_media/d1-h_user_manual_v1.0.pdf#page=174)

#### 其他 PHY 的寄存器和阻抗对照

V853 Tina SPL 的 `board/v853/libsun8iw21p1_sdcard.a` 也使用 `0x7b7bfb` 作为 792 MHz DDR3 默认值。`mctl_channel_init` 的 `0x15e–0x17a` 与 T113 使用相同的地址 `0x03103140`、24 位掩码和 bit 25。V853 数据手册 §2.3.7 表 2-9 列出接收端 ODT 的开路、120 Ω 和 60 Ω，未列输出驱动档位或 ZQCR 编码。 [V853 Tina SPL 库](https://gitee.com/xiaobocwb/tina-spl-pub/tree/f436101c3dad52769e8e338bc207941a0e0df3b0/board/v853) · [V853 数据手册 V1.2 表 2-9](https://bbs.aw-ol.com/assets/uploads/files/1661647385303-fd703522-58f1-435d-986f-c35f2843a2c5-v853-amp-v853s_datasheet_v1.2.pdf#page=30)

H3、A64、H5、R40、V3s 的共享 U-Boot 驱动与 T113 的 `ZQCR`、`ZQSR` 和三份 `ZQDR` 偏移一致。H3 的逐字段校准路径把每个半字节复制到六个位置后单独校准，再按以下顺序写回校准结果。A64、H5 和 R40 的路径一次写入低 24 位，由 PHY 完成校准。 [共享寄存器表](https://github.com/u-boot/u-boot/blob/v2026.10/arch/arm/include/asm/arch-sunxi/dram_sunxi_dw.h#L119-L121) · [H3 校准](https://github.com/u-boot/u-boot/blob/v2026.10/arch/arm/mach-sunxi/dram_sunxi_dw.c#L329-L359) · [A64/H5/R40 校准](https://github.com/u-boot/u-boot/blob/v2026.10/arch/arm/mach-sunxi/dram_sunxi_dw.c#L575-L590) · [T113 寄存器表](https://github.com/OpenCentauri/oc-boot/blob/fca7c0a5b5b3fd4f2d619a1e24dfccd1af5dd8da/reg-dram.h#L76-L80)

| ZQCR 位 | `0x7b7bfb` 的值 | H3 对应的校准结果寄存器 |
|---|---|---|
| `[7:4] / [3:0]` | `f / b` | `ZQDR0`，偏移 `0x148` |
| `[15:12] / [11:8]` | `7 / b` | `ZQDR1`，偏移 `0x14c` |
| `[23:20] / [19:16]` | `7 / b` | `ZQDR2`，偏移 `0x150` |

T113-i 硬件设计指南 §1.5.2 定义了实际引脚组。DX0 为 DQ0–7、DQM0、DQS0P/N，DX1 为 DQ8–15、DQM1、DQS1P/N。指南另列 Command & Address，规定主控和颗粒各自使用 240 Ω ±1% 的 ZQ 参考电阻。当前网表符合该阻值。指南与 SDK 都未给出 `ZQDR0/1/2` 到 DX0、DX1、命令地址或时钟引脚的对应关系。T113-i 用户手册 V2.2 §7.1 给出 DRAMC 的位宽、片选及支持的 DDR 类型。 [T113 硬件设计指南 V1.0](https://whycan.com/files/202304/T113-I/T113_%E7%A1%AC%E4%BB%B6%E8%AE%BE%E8%AE%A1%E6%8C%87%E5%8D%97V1_0.pdf#page=18) · [T113-i 用户手册 V2.2](https://www.scribd.com/document/995591368/T113-i-Series-User-Manual-V2-2)

STM32MP1 表 9 给出低半字节 `0x9` = 48 Ω、`0xb` = 40 Ω、`0xf` = 30 Ω。TI Keystone II 的低半字节也以 `0xb` 选 40 Ω，其支持表未列 `0x9`。两家高半字节 `0x2` 的 ODT 分别为 96 Ω 和 120 Ω。H6 的 `ZQCR` 则在 PHY 偏移 `0x680`，U-Boot 将同名 `CONFIG_DRAM_ZQ` 参数拆写到 `ZQ0PR0`、`ZQ0OR0` 和 `ZQ1PR0`。这些差异限制了跨芯片套用编码。若 T113 的每个字节都沿用 ST 的低半字节定义及换算，则 `0x7b7bfb` 的三个 `b` 表示 40 Ω，同时改成 `9` 得到候选 `0x7979f9`。SD/FEL SPL 仍使用 `0x7b7bfb`，等待同型号 PHY 位域与 Ω 表，或实板分别确认引脚映射和阻抗。 [ST AN5168 表 8–9](https://www.st.com/resource/en/application_note/an5168-ddr-configuration-on-stm32mp1-series-mpus-stmicroelectronics.pdf#page=16) · [TI SPRUHN7C 表 4-61](https://www.ti.com/lit/ug/spruhn7c/spruhn7c.pdf#page=117) · [H6 ZQ 设置](https://github.com/u-boot/u-boot/blob/v2026.10/arch/arm/mach-sunxi/dram_sun50i_h6.c#L487-L498)

调研源码在 WSL 的 `~/work/KVMPanel/upstream/` 下，目录为 `tina-spl-pub-mirror`、`tina-mangopi-mirror` 和 `xfel`。全志手册、D1 板级配置和 Centauri Boot0 在 `allwinner-ddr-reference`。反汇编文件在 `~/work/KVMPanel/build/t113-zq-research/`，包括 `sun20iw1p1-sdcard.lib.txt`、`r528-sdcard.lib.txt`、`centauri-boot0.thumb.txt` 和 `v853-sdcard.lib.txt`。

## 先从 RAM 加载 Linux

本项目使用主线 U-Boot 的 R528 DDR3 实现及上述板级值，用 USB FEL 在 RAM 中运行 SPL 和 U-Boot，再传入 `zImage`、本板 DTB、Buildroot `rootfs.cpio.gz`。网表确认 USB1 直连 SoC USB0，USB2 经 CH343K 接 UART0 PB8/PB9。上板后检查 SPL 训练、内存容量、内存读写和内核的 CPU 上线数。 [FEL 启动配置](../../u-boot/README.md)

## microSD 冷启动

2026-11-22 版以 CARD1 microSD 为板载启动介质。验证顺序是 FEL 加载到 RAM、核对 SMHC0 的只读访问，再验证从 SD 装载 SPL、U-Boot 和 Debian。镜像使用 MBR 和两个 ext4 分区，冷启动待实板确认。 [Debian 镜像](../../debian/README.md) · [板级接线](../../../../hardware/README.md)

## 旧版的 SPI NAND 调研

本版已移除板载 SPI NAND。下列记录保留 Tina 参考启动链与驱动调研结果。

- **Tina 的 NAND 读取。** 参考链路是 `BROM → boot0_nand → boot_package → U-Boot → boot`。NAND 方案的 `storage_type=5`。打包程序用本板 `sys_config.bin` 更新 Boot0，U-Boot 包含在 `boot_package.fex`。参考启动环境先用 `sunxi_flash read 43000000 boot` 读逻辑 `boot` 分区，再用 `bootm 43000000` 加载 Android 格式的 `boot.img`。参考固件归档里同时有 SD 与 NAND Boot0，包内 `sys_config.fex` 的 `storage_type=1`，上面的 NAND 环境来自 `evb1_auto_nand` 源码。 [Tina 打包框架](https://github.com/100askTeam/Tina5-LinuxSDK) · [NAND 参考板配置](https://github.com/DongshanPI/T113i_DevKitF_Tina5SDK/tree/master/device/config/chips/t113_i/configs/evb1_auto_nand)
- **W25N04KWZEIR。** Linux 6.18.64 和主线 U-Boot 2026.10 的 SPI NAND 设备表均匹配 `EF BA 23`，几何为 2048 B 数据页＋128 B OOB、64 页/擦除块、4096 块，ECC 为 8 bit/512 B。参考 `boot0_nand.fex` 和 Tina U-Boot 中未找到这个 ID 的连续字节，参考 U-Boot 中可见 W25N01GV 与 W25N02KV。主线 U-Boot 的 SPI NAND 设备表只证明 U-Boot proper 有对应驱动。Sunxi SPL 的专用 SPI 加载路径读取 NOR，无法据此确认 NAND 冷启动。
