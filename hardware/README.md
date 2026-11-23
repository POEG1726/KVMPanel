# 硬件

- `eda/`：存放原生工程。
- `exports/`：存放原理图、网表和 BOM。
- `reference/`：芯片手册和外部参考资料。

[TD4322.zip](reference/display/TD4322.zip) 是卖家提供的拆机屏参考包，含面板及 S3508 触摸驱动示例、SGM3851A 手册和连接器针脚图。实际面板型号待确认。

RTL8211F-CG 的新版方案已确定为本地 25 MHz 无源晶体，设备树已按该方案配置。下表保留 2026-11-22 导出的接线，新版网表待归档。 [网口说明](../docs/development.md#单独验证本板网口)

## 2026-11-22 本板导出

- [原理图 PDF](exports/2026-11-22/SCH_Schematic2_2026-11-22.pdf)（T113-Core、Power、Peripheral）
- [嘉立创 EDA ENET 网表](exports/2026-11-22/Netlist_Schematic2_2026-11-22.enet)

| 接口 | 图纸和网表中的接线 |
| --- | --- |
| 调试串口 | U18 的 UART0 PB8/TX、PB9/RX 分别接 U11（CH343K）的 RXD、TXD。调试口是 USB2（`AUX-PWR / DBG`）。 |
| FEL | BOOT 键将 U18.A18（FEL）拉到 GND，RST 键接 U18.M2。USB1（`Download / COMM`）的 D+/D− 直连 SoC USB0，同时接到 JAF2。 |
| DDR | U22 是单颗 `K4B4G1646D-BCK0`，有 16 根 DQ。U22.L2（CS#）按设计固定接地，U18.R12（SCS0）悬空。U22 与 SoC VCC-DRAM 同接 `1V5`，VDD18-DRAM 接 `1V8`。 |
| 存储 | microSD CARD1 的 DAT0–3 接 PF1、PF0、PF4、PF5，CLK/CMD/CD 接 PF2/PF3/PF6。卡座 VDD 接 `3V3_MAIN`，VCC-PF 接 `1V8`。本版已移除板载 SPI NAND。 |
| USB 供电 | USB1 和 JAF2.11 接 `MB_5V`，USB2 接 `AUX_5V`。TPS2116 U12 从两路输入选择板上 `5V`，USB-A U16 用该输出供电。 |
| MIPI DSI 与触摸 | CN2 接 PD0–PD9 的四组数据对和一组时钟对。DISP_RST/TE/ERR 分别接 PD18/PD15/PD16，TOUCH_RST/IT 分别接 PD19/PD14。VCC-LVDS 接 `1V8`，VCC-PD 和触摸 I²C 上拉接 Q10 切换后的 `VCC_PD_1V8`。PD12/PD13 接 SGM3851A 的 EN_VO3/CTRL，对应 CN2 的 VDDP_EN/ELON2 经 R17/R13 接入。 |
| HDMI 采集 | LT8619C 的 D8–D15 接 PE4–PE11，控制 I²C 接 PE14/PE15。RESET_N 接 PC3 并经 R302 下拉，GPIO15 接 PE17。 |
| 网络和 USB Host | RTL8211F 接 RGMII，PG14/PG15 分别为 MDC/MDIO，PG11 提供 25 MHz 参考时钟。PHY_RESET_N 接 PC1 并有 RC，PHY_INT_N 接 PC7。VCC-PC、VCC-PG 接 `3V3_MAIN`。SoC USB1 接 USB-A U16。 |
| 主板控制 | GAQY212GSX 光继电器的控制信号为 PE13 电源键、PE3 复位键、PB2 SAFEBOOT、PB7 BCLK+、PB4 BCLK−，均为高有效。电源灯输入 PE12 由 U7 输出拉高，R37 下拉。HDD 灯线路在 H1/H3 直通。 |
| 实体键和 STC | H7.1 的 KEY_ADC 直连 U25.P3.2，经 Q1 接 T113-i 的 GPADC0。PB0/PB1 通过 D2/D1 连接 STC UART。Power 页标注首烧时暂不装 R34、D2、R32。 |
