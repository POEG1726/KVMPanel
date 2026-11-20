# 硬件

- `eda/`：存放原生工程。
- `exports/`：存放原理图、网表和 BOM。
- `reference/`：芯片手册、PCB 器件清单和外部参考资料。

`reference/board-components-20261120.json` 是当日打开 PCB 的器件属性清单，不代表实物已装配，也不能当网表使用。

[TD4322.zip](reference/display/TD4322.zip) 是卖家提供的拆机屏参考包，含面板及 S3508 触摸驱动示例、SGM3851A 手册和连接器针脚图。实际面板型号待确认。

## 2026-11-20 本板导出

- [原理图 PDF](exports/2026-11-20/SCH_Schematic2_2026-11-20.pdf)
- [TEL 网表](exports/2026-11-20/Netlist_Schematic2_2026-11-20.tel)

| 接口 | 图纸和网表中的接线 |
| --- | --- |
| 调试串口 | U18 的 UART0 PB8/TX、PB9/RX 分别接 U11（CH343K）的 RXD、TXD。调试口是 USB2（`AUX-PWR / DBG`）。 |
| FEL | BOOT 键将 U18.A18（FEL）拉到 GND，RST 键接 U18.M2。USB1（`Download / COMM`）的 D+/D− 直连 SoC USB0，同时接到 JAF2。 |
| DDR | U22 是单颗 `K4B4G1646D-BCK0`，有 16 根 DQ。U22.L2（CS#）按设计固定接地，U18.R12（SCS0）悬空。U22 与 SoC VCC-DRAM 同接 `1V5`，VDD18-DRAM 接 `1V8`。 |
| SPI NAND | U5 确认为 `W25N04KWZEIR`，按 Quad SPI 接线。PC2 为 CLK，PC3 为 CS#，PC4–PC7 为 IO0–IO3。Flash VCC 和 SoC VCC-PC 同接 `1V8`。R47 和 R46 将 PC4 和 PC5 各经 3.3 kΩ 下拉，默认绑带为 `00`。 |
| MIPI DSI | CN2 接 PD0–PD9 的四组 DSI 数据对和一组时钟对。SoC 的 VCC-PD、VCC-LVDS 同接 `1V8`。当前网表中 DISP_RST、DISP_TE 和 DISP_ERR 只到 CN2。VDDP_EN、ELON2 由 CN2 经过电阻接入 SGM3851 的控制线路。 |
| 网络和 USB Host | RTL8211F 接 RGMII，MDC/MDIO 分别接 PG14/PG15。25 MHz 参考时钟由 SoC 的 PG11（U18.D4）提供，PHY 复位用 RC。SoC USB1 接 USB-A U16。 |

### 制板前要核对的启动项

- **U5 贴装资料和 IO 偏置。** 归档 TEL 的 `$PACKAGES` 仍显示 `GD5F1GQ4UFYIGR`，贴装用 `W25N04KWZEIR`。CS# 已有 R48 上拉。该料号出厂 `WP-E=0`、`SRP=00`、`H-DIS=1`，Quad 操作时 IO2/IO3 传输数据。若启用 HOLD，Winbond 要求主机驱动 IO3 为高或用外部上拉，避免低电平在非 Quad 指令期间暂停传输。弱上拉可为高阻时段提供偏置，BootROM 阶段的引脚电平和命令仍需实测。 [Winbond 手册 4.3、4.4、7.2.6 及图 11](https://gr.rxelectronics.com/datasheet/64/w25n04kwtciu-tr.pdf)
- **双 USB-C 的供电。** USB1、USB2 和 USB-A U16 的 VBUS 共用同一 `5V` 网。首次同时连接 FEL 和调试串口前，应确认供电隔离，避免两个 USB 供电源通过板上 5 V 并接。
