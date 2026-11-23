# T113-i / Linux 6.18 适配检查

日期：2026-11-20。检查基线：v6.18 与 v6.18.64。

板级接线已按 2026-11-22 导出复核。以下构建与 QEMU 结果记录软件验证，板上接口状态以新版接线为准。 [硬件说明与引脚分配](../hardware/README.md)

2026-11-23 在 Ubuntu WSL 的 Linux 6.18.64 上合并构建了 RTL8211F-CG、SGM3851A 和 LT8619C 的改动，生成板级 `zImage` 和三份 DTB。三个驱动对象通过 `W=1` 编译，LT8619C 和 SGM3851A 也单独链接为 ARM 模块。组合 DTB 通过四个相关绑定的 `dt-validate`。构建记录位于 WSL 的 `~/work/KVMPanel/build/linux-phy-display-integration/`。既有 QEMU virt 记录覆盖用户态、USB 枚举和先前版本的 LT8619C、CSIC、TD4322 模块装卸。本次改动待实板验证。 [构建与测试命令](development.md#构建应用所需的内核模块和组合-dtb)

## 结果

| 功能 | 电脑上已核对或验证 | 待实板验证 |
|---|---|---|
| 板级 DTS / 启动 | 基础 DTS 加显示、采集与外设，保留 FEL 和 initramfs 启动 | FEL 启动、实际板型、引脚电压 |
| DDR / 双核 / 串口 | U-Boot 有 T113-i 外置 DDR3 参考，QEMU 以双 Cortex-A7 登录 | DDR 训练、512 MiB、PSCI、UART0 |
| 网络 / USB-A | EMAC 已启用，使用 RTL8211F-CG 本地 25 MHz 无源晶体、PC1 复位和 PC7 中断。既有 QEMU 记录覆盖 DHCP、网关和 USB 存储读取 | 晶体起振、复位及电源波形、MDIO、RGMII 延时和真实 USB-A 收发 |
| USB0 gadget | 默认 3 HID、3 LUN、EP0 遥测通过 QEMU，默认 4 IN / 1 OUT | MUSB、FIFO 分配、BIOS 键鼠与 Windows 兼容性 |
| microSD | CARD1 接 SMHC0，卡电源固定 3.3 V，PF 信号由 MMC 栈协商切换。已启用 UHS-I，SDR104 卡时钟上限 150 MHz | 测 PF 电压与时钟，核对 CD 极性，确认只读识别、稳定性和 SD 冷启动 |
| LT8619C / CSI | PC3 复位、I²C 配置、PE17 全局中断和 GPIO13 的 HDMI 5 V 读取已编译。保留标准 V4L2 子设备和独立 CSIC 驱动 | 复位和中断波形、5 V 检测、HPD/EDID、DMA、持续采集与换模式 |
| JPEG / MJPEG | 软件 NEON 编解码通过；硬件 ABI 已调查 | 硬件编码节点、真实吞吐、CPU 占用和采集并发 |
| TD4322 / DSI / 背光 | 参考初始化、TE 和 DCS 亮度已编译，面板节点禁用。SGM3851A 默认开启三路输出，SWIRE 电压接口已实现并通过编码测试 | 测三路输出和 CTRL 波形，确认面板型号、电平、极性与时序 |
| S3508 / BH1750 | I²C2 节点与内核驱动就绪；新版触摸 IRQ 接 PD14，S3508 节点禁用 | 确认触摸型号与 IRQ 极性，验证 F12 坐标和光照读数 |
| 主板控制 / 实体键 | 已核对光继电器 GPIO 和 H7 的 KEY_ADC 线路，板级 DT 已启用 GPADC0 | 核对 GPIO 电平、ADC 电压、按键映射与 STC 交接 |
| 温度 / 看门狗 | D1 THS、SID 校准、thermal zone、系统 WDT 节点已编译 | 校准读数、固件所有权、喂狗与受控复位 |
| 自身关机 | STC 板级关机通信沿用既有待办，本计划六项未定义协议 | 核对协议、下电和恢复时序 |
| C906 / HiFi4 | remoteproc、mailbox、RPMsg 和两份回环固件已构建并打包 | 两核的时钟、复位、共享内存可见性、独立及同时回环 |

## C906、HiFi4 与 GPADC

C906 和 HiFi4 由 A7 上的 Linux remoteproc 加载。板级 DT 使用 CPUX mailbox 的通道 4 和 0，分别为 C906 和 HiFi4 预留固件、vring 和 RPMsg buffer。C906 固件从 `0x4fa00000` 加载，HiFi4 指令在 `0x00400000` 的 IRAM，数据从 `0x4fc00000` 加载。驱动沿用 MiniEVM 的寄存器、时钟和复位路径，内存分配使用本板的固定 DT carveout。默认手动启动。 [构建与上板命令](development.md#c906-和-hifi4) · [MiniEVM 驱动来源](https://github.com/nickfox-taterli/t113-linux/tree/4e1c29c35)

三个驱动通过 ARMv7 `W=1` 编译，最终内核和板级 DTB 已生成。已核对两份 ELF 的加载段、RPMsg resource table 与 DT 的地址和大小，预留区和本次 FEL 镜像互不重叠。QEMU 的板外测试通过，覆盖 Buildroot 镜像启动、固件目录、诊断程序和既有基础服务。两个核的复位、时钟和 RPMsg 收发待在实板验证。

H7.1 的 `KEY_ADC` 直连 STC 的 P3.2，经 Q1 连到 T113-i 的 `GPADC0`（`T113_ADC`），`GPADC1` 悬空。板级 DT 和内核配置开放 GPADC0 供 Linux 读取。按键板电阻和 Q1 两侧电压实测后，再为 FUNC 和普通按键设置 `adc-keys` 阈值。GPADC 输入满量程以 AVCC 为上限。 [新版网表](../hardware/exports/2026-11-22/Netlist_Schematic2_2026-11-22.enet) · [T113-i 数据手册 §5.7](../hardware/reference/datasheets/T113-i_Datasheet_V1.4.pdf)

## MIPI DSI

- 可独立构建的 [DSI 设备树](../software/os/linux/dts/sun8i-t113i-kvmpanel-dsi.dts) 启用 DE → TCON LCD0 → DSI，引用 SoC 的 PD0–PD9 pinmux、D-PHY 和本板固定 1.8 V 电源。基础 DSI DTS 用于主控单测。组合 DTB 另加 TD4322 子节点。
- [时钟补丁](../software/os/linux/patches/0001-sun6i-dsi-tcon-top-clock.patch) 将 D1/T113 共用的 A100 路径的模块时钟获取移到 component bind 阶段，等待 TCON TOP 提供该时钟。[Generic Write 补丁](../software/os/linux/patches/0002-sun6i-dsi-generic-write.patch) 支持通用短、长写，沿用现有 LP 发送路径供面板初始化使用。 [时钟补丁原帖](https://lists.openwall.net/linux-kernel/2026/09/26/448)
- [卖家参考包](../hardware/reference/display/TD4322.zip) 的面板驱动采用 4 lane、RGB888、1080×1920、140.640 MHz 像素时钟和 Command Mode，并在初始化发送 Generic Long Write。原版 sun6i 像素输出走 Video Mode，Generic Write 只提供初始化包传输，本板 0004 系列补丁另行实现 Command Mode 帧更新和 TE 同步。现有 DCS 读路径只支持回读一个字节。
- 参考包针脚图的四组数据对和时钟对与 CN2 网表的信号名称、lane 顺序和极性相符。现版将 DISP_RST、DISP_TE、DISP_ERR 分别连到 PD18、PD15、PD16。面板类型、供电时序和复位方式需要结合拆机屏实物确认。

### TD4322 刷新与背光

参考包的 4 lane RGB888、1080×1920 和 140.640 MHz 模式已编进面板驱动。1302×1957 总时序折合约 55.20 Hz，计算线速约 843.84 Mbit/s/lane。面板初始化包的类型、字节和延时按卖家序列复核。有效的外部 TE 边沿触发 HSTX 和 TCON；连续两个名义帧周期未触发时，看门狗按帧周期尝试发送真实帧。共用的触发路径会跳过在途帧，并滤掉半帧内的重复边沿；每行传输 3240 字节像素，首行写 0x2c，后续 0x3c。TCON TRI_FINISH 上报 vblank。LP 控制命令与帧传输互斥，忙时跳过该 TE。

亮度用 DCS 0x51 暴露为 0–255 的 backlight。PD12（EN_VO3）、PD13（CTRL）和 CN2 经 R17/R13 共用 SGM3851A 的两条控制线路。组合 DTB 保留 panel@0 并禁用。确认面板型号、VCI/VDDI、PD18 的复位极性和脉宽、PD15 的 TE 极性后，填入 reset-gpios 与 te-gpios 并启用。上板测频，并验证黑白与色条、亮度、持续翻页和休眠恢复。 [卖家参考包](../hardware/reference/display/TD4322.zip) · [BSP 寄存器与触发路径](https://github.com/AvaotaSBC/linux/tree/7c2a33957cb82e522885992ec26331d1c1c6db28/bsp/drivers/video/sunxi/disp2/disp/de/lowlevel_v2x)

### SGM3851A 电源和 SWIRE

组合 DTB 下，驱动请求 3V3_DISP，保持 PD12（EN_VO3）和 PD13（CTRL）同时为低至少 100 µs，再先后拉高两脚。关机时将两脚拉低至少 100 µs。芯片 VIN 冷启动的默认输出为 ELVDD +4.6 V、ELVSS −2.5 V、AVDD +6.1 V。板级设备树省略可选的 `sgmicro,swire-pulses`，启动时没有编程脉冲。

通过 `/sys/bus/platform/devices/pmic/voltages_uv` 一次写入三个有符号微伏值，顺序为 `ELVDD ELVSS AVDD`。驱动先按手册表 1 校验整组，再只为变化的输出发命令。读取返回驱动记录的目标电压。若 I/O 失败造成设定状态未知，读取返回错误。 [驱动与绑定](../software/os/linux/patches/0007-regulator-sgm3851a.patch) · [SG Micro 手册](https://www.sg-micro.com/rect/assets/28610e17-9fe0-4f6d-afeb-2e8defd416f3/SGM3851A.pdf)

| 输出 | 运行时支持值 | 出厂默认 |
|---|---|---|
| ELVDD | +4.7～+5.0 V，步进 0.1 V | +4.6 V |
| ELVSS | −5.4～−1.4 V，步进 0.1 V | −2.5 V |
| AVDD | +5.8～+7.9 V，步进 0.3 V | +6.1 V |

```sh
cat /sys/bus/platform/devices/pmic/voltages_uv
printf '%s\n' '4600000 -2500000 6100000' > /sys/bus/platform/devices/pmic/voltages_uv
```

示例保持出厂设定。只要 ELVDD 仍为 +4.6 V，就可沿用该值并调整其他两路。调高 ELVDD 后，表 1 没有恢复到 +4.6 V 的 SWIRE 命令，需要让 SGM3851A 的 VIN 重新上电。若刚调整过电压，热重启或重新绑定驱动前先让 VIN 完整掉电，保证硬件回到驱动的默认起点。首次验证先断开面板，因为 CN2 与 PMIC 经 R17/R13 共用控制线。驱动在使能后等待至少 450 µs，帧内高低电平各 10 µs，帧间保持高电平至少 80 µs。实板用逻辑分析仪核对 PD13 的脉宽及上升沿数，同时测三路输出电压。离线编码测试覆盖表 1 的全部 53 个命令、ELVDD 默认值及 65 个非法输入。

## USB0 gadget

板上运行 `kvmpanel-gadget --help` 查看组合与镜像参数。默认键盘、相对鼠标、绝对鼠标各一个 HID，存储使用一个 bulk 功能。LUN0 是只读光驱，LUN1 是虚拟 U 盘，LUN2 是共享盘。介质路径可留空，两个磁盘槽默认只读。启用写入前应先卸载同一镜像的本地文件系统。

每个绝对鼠标 Input 报告都是 7 字节，首字节为 report ID `0x02`，后接 6 字节鼠标数据；单独或合并鼠标、开关遥测时均相同。每个厂商遥测 Output 报告都是 64 字节，首字节为 `0x03`，后接 63 字节数据。启用绝对鼠标时两者共用接口。关闭绝对鼠标且启用遥测时，遥测独占一个 IN，相对鼠标保持无编号的 boot 报告（subclass 1、protocol 2）。默认组合仍使用独立的无编号 boot 相对鼠标。HID 的 `no_out_endpoint=1` 使主机的 SET_REPORT 走 EP0。QEMU host 读到三个独立 LUN 的标记，并从 hidraw 到 /dev/hidgN 逐字节比对了默认的 64 字节报告。NCM、RNDIS、ACM 的合法组合已解绑重建并配置成功。

T113-i 手册 USB2.0 OTG 一节给出 EP1–EP5 各一个 IN、一个 OUT，FIFO 共 8 KiB + 64 B。默认组合实测用 4 IN / 1 OUT。6.18.64 的 sunxi MUSB 五端点表按每方向 512 B 单缓冲配置。脚本在绑定前计算预算，dummy_hcd 无法测出 T113-i 的端点上限或 FIFO 行为。MUSB、冷启动 BIOS、Windows hidapi 和实际多 LUN 主机兼容性待实板验证。

## LT8619C 和并行 CSI

U26 的 D8–D15 接到 CSI_D0–7，控制 I²C 使用 PE14/PE15、七位地址 0x32。Linux 探测时保持 PC3（LT_RESET_N）为低 100 ms，释放后等待至少 50 ms，再通过 I²C 识别芯片、初始化接收器、写入 EDID，并控制 HPD 和 BT.656 输出。默认 EDID 1.3 首选 1080p30，另含 720p60 和 VGA。寄存器 0x6089 选择内部参考，REXT 接 1.8 V。配置随驱动启动写入芯片寄存器和 EDID RAM。 [Rockchip 源码](https://github.com/rockchip-linux/kernel/blob/develop-6.1/drivers/media/i2c/lt8619c.c) · [Lontium 产品资料](https://www.lontiumsemi.com/uploadfile/202410/9301611728be52d.pdf)

GPIO15 复用成低有效全局中断，接到 T113 的 PE17。线程通过 I²C 读取并清除视频时钟、同步和 VIC 标志，再通过 V4L2 子设备上报时序变化。驱动在连续清除失败或中断没有匹配标志时停用 IRQ，用户空间仍可查询时序。GPIO13 设为输入，接 HDMI 5 V 分压，V4L2 的 `rx_power_present` 每次读取当前电平。已取得的寄存器表没有 GPIO13 的独立事件，5 V 有无通过该只读控件查询。清除位的低有效脉冲根据寄存器复位状态推断，上板先核对 PE17 的极性及清除后的电平。 [接收器驱动](../software/os/linux/patches/0003-2-media-capture-drivers.patch) · [寄存器表与参考程序归档](https://gitcode.com/open-source-toolkit/082c0)

独立的 sun20i-csi 采用社区 D1 CSIC 驱动，经 Tina VIN 和手册核对 TOP、parser、DMA、IRQ；DMA0/1 的 GIC SPI 为 95/96，parser0 为 100。板级链路用单通道 BT.656 嵌入同步、8 位 SDR 和 UYVY8_2X8，捕获队列使用 VB2 DMA 连续缓冲。开始上板验证时可从 CMA 32 MiB、4 个 VGA 缓冲起步。[CSIC 原补丁系列](https://lore.kernel.org/linux-media/20260930154835.402684-1-tien.nguyenminh@embeddedlinux.blog/) · [寄存器手册 §6.1](https://dl.linux-sunxi.org/T113-S3/T113-S3_User_manual_v1.1_20210830.pdf)

8 位 SDR 在一个 PCLK 周期传 1 字节，YUV 4:2:2 每像素占 2 字节。T113-i 手册表 5-21 的时钟上限 148.5 MHz 对应 148.5 MB/s 或 74.25 Mpixel/s。以下用十进制 MB，标准 CTA 总时序为 2200×1125。[本板 SoC 数据手册](../hardware/reference/datasheets/T113-i_Datasheet_V1.4.pdf)

| 模式 | 有效画面载荷 | 含消隐所需字节时钟 | 结论 |
|---|---:|---:|---|
| 1920×1080p60 | 248.832 MB/s | 297 MHz | 超过上限，削减软件帧率也不能接收这一输入时序 |
| 1920×1080p30 | 124.416 MB/s | 148.5 MHz | 理论可行，正好到上限；边沿裕量、像素顺序、丢帧待实板验证 |

上板先用 `media-ctl -p` 识别 LT 子设备和捕获节点。插拔 HDMI 时对照 `v4l2-ctl -d /dev/v4l-subdevN --get-ctrl=rx_power_present`、`/proc/interrupts` 和 PE17 波形。读取 `v4l2-ctl -d /dev/v4l-subdevN --query-dv-timings`，锁定后执行 `--set-dv-bt-timings query`。以实际宽高设置 CSI sink，例如 `media-ctl -V '"sun20i-csi-bridge":0 [fmt:UYVY8_2X8/640x480 field:none]'`，再用 `v4l2-ctl -d /dev/videoN --set-fmt-video=width=640,height=480,pixelformat=NV12 --stream-mmap=4 --stream-count=120 --stream-to=/dev/null` 验证。应用可订阅接收器子设备的 `V4L2_EVENT_SOURCE_CHANGE`，拔线或换模式后查询 DV timings，再 STREAMOFF、释放缓冲、重新设置时序和格式、重新排队。

## JPEG/MJPEG 编码

T113-i 数据手册 §2 的 VE 规格列出 JPEG/MJPEG 编码最高 1080p60；该指标描述编码器能力。本板 HDMI 输入另受上述 8 位总线限制。6.18.64 的 Cedrus 匹配 sun20i-d1-video-engine，cedrus_video.c 的输出队列接收压缩切片、采集队列给出原始图像。本板没有可直接使用的主线 V4L2 JPEG 编码节点。[Cedrus 队列实现](https://github.com/gregkh/linux/blob/v6.18.64/drivers/staging/media/sunxi/cedrus/cedrus_video.c)

社区作者在 Linux 6.12.28 的 stateful V4L2 M2M 补丁上报告了 T113 JPEG 成功。6.18.64 仍需移植该补丁、核对 VE 的时钟/复位/电源域与 DMA 地址，并用实际原始帧验证 JPEG 输出。附件请求返回 HTTP 403，本次没有取得可验证应用的补丁正文。厂商 Cedar ABI 则要配套 /dev/cedar_dev、内核 ioctl、可供 VE 使用的 DMA 内存与匹配的 libvencoder/JPEG 库；Tina DTS 使用 sunxi-cedar-ve，与主线 Cedrus 的 V4L2 ABI 不同。目前没有在本项目的 ARMhf 静态 musl 系统验证配套库和设备驱动。两条硬件路径均留待实板。[补丁作者的测试说明](https://forum.armbian.com/topic/26551-video-encoder-using-mainline-kernel/) · [Cedar 设备 ABI 实现](https://github.com/uboborov/sunxi-cedar-mainline) · [Allwinner 编码示例](https://github.com/FREEWING-JP/OrangePi_CedarX/blob/master/cedarx/demo/demoVencoder/EncoderTest.c)

保底方案为 Buildroot 的 libjpeg-turbo 2.1.5、Cortex-A7 和 NEON/VFPv4。核实了 CMake WITH_SIMD=ON、libjpeg.a 中的 NEON FDCT/降采样符号，并在 QEMU Cortex-A7 编解码了一张 16×8 色块图，核对了回读尺寸和像素误差。1080p 的实际软件帧率、CPU 占用和采集叠加时的内存带宽待实板验证。

## 触摸、光照、SD 和 GPIO

I²C2 的 PD20/PD21 分别通过 U18.W18/V18 接 SCL/SDA。总线经 R19/R18 上拉到 Q10 切换后的 `VCC_PD_1V8`。BH1750 U6.2 的 ADDR 接地，7 位地址为 0x23。S3508 卖家参考地址为 0x20，采用 RMI4 I²C/F12。CN2.35 TOUCH_IT 接 PD14，CN2.5 TOUCH_RST 接 PD19，触摸节点暂时禁用。待实板核对 S3508 型号、IRQ 电平、触点方向与 BH1750 的 IIO 读数。

CARD1 的数据、时钟和命令接 PF0–PF5，CD 接 PF6。组合 DTB 启用 `mmc0`，用轮询检测插拔，待确认 CD 极性后可改用 PF6。卡电源 `vmmc` 固定为 `3V3_MAIN`，U18.F4（VCC-PF）接 `1V8`。板级 pinctrl 的 `pf-vqmmc` 选择 PF 信号电压，由 MMC 栈在卡时钟停止后请求。

卡从 3.3 V 初始化。只有卡接受 1.8 V 协商且完成 CMD11 时才切低，并用 PIO 0x0348 校验 PF 电压状态。切换超时会尝试恢复旧档并返回失败。卡的 VDD 直连 3.3 V，切换中断后若卡需要完整掉电复位，需拔插卡。SDR104 尝试最高 150 MHz，扫描 64 个采样延迟档位并重验最长通过窗口的中点。调谐失败时以最高 100 MHz、50 MHz 继续试同一 UHS 模式。普通卡继续按其支持的默认速度或 High Speed 运行。

下面保留实现前的 PF 电路查证。实板先读 `/sys/kernel/debug/mmc0/ios`，同时测 CMD/DAT 电平和 SD_CLK。FEL 和 initramfs 启动可用于独立验证 SD。

Linux 6.18.64 的 `zImage`、模块和组合 DTB 已构建，已检查 SD 与 SGM3851A 的设备树绑定及内核符号。PF 电压、SD 调谐窗口及 PMIC 波形待实板测量。

PF 组 IO 电源在片内可切换。T113-S3 用户手册 V1.5 第 9.7.5.67 节有 `PIO_POW_VOL_SEL_CTL`（PIO 基址 0x0200_0000 + 0x0350），bit0 为 VCC-PF Power Voltage Select，0 为 1.8 V，1 为 3.3 V，复位默认 1。第 9.7.5.64 节 0x0340 的 PF_PWR_MOD_SEL（bit5）和 0x0348 的 PF_PWR_VAL 都注明“PF 口电源源选 VCC_IO 时该位无效”，但 PC/PD/PE/PG 各位有同一句话，是通用注释，不能单独说明 PF 有片内电源切换。手册没有画内部电路，也没写 1.8 V 来自哪里。“3.3 V 档接片内 VCC-IO、1.8 V 档用 VCC-PF 球”是推断，依据是 0x0350 只针对 VCC-PF、默认 3.3 V、硬件指南写 T113-S3 的 VCC-PF 片内接 VCC-IO，以及 Tina `sun8iw20p1.dtsi` 的 `vcc-pf-supply = 1.8 V`、`vcc-pfo-supply = 3.3 V`（`pfo` 含义无文档）。公开参考板（T113-i EVB、NezhaSTU 核心板）都把 VCC-PF 接 3.3 V，没有接 1.8 V 的先例。T113-S3 无 VCC-PF 球，PF 只能走 VCC-IO；T113-i 的 EVB 把 VCC-PF 也接 3.3 V，只用 3.3 V 档。[T113-S3 用户手册](../hardware/reference/datasheets/T113-S3_User_Manual_V1.5.pdf)

手册里与 PF 供电相关的其他内容：
- 电源域：硬件指南 1.16 节写有独立电源脚的 GPIO 组用该脚供电，没有的（PB）用 VCC-IO。T113-i datasheet 表 4-2 写 PF0–PF6（球 C2、C1、D2、D1、E3、E2、D3）的 IO buffer 电源都是 VCC-PF（球 F4）。表 5-3 写 VCC-PF 可用 1.8 V（1.62–1.98）或 3.3 V（2.97–3.63），表 5-4 写最大 50 mA。上电时序图把 VCC-PF 和 VCC-IO、PD/PE/PG 放在同一组，只要求 VCC-IO 比 VDD-SYS/VDD-CPU 早至少 2 ms。T113-S3 用户手册表 9-20 写 PF 只有 3.3 V，因为 S3 封装里 PF 接 VCC-IO。
- 引脚结构：用户手册图 9-67/9-68/9-69 是所有 GPIO 共用的结构图。输入是带 En_in 的施密特缓冲；输出是 4 路并联推挽，每路典型 180 Ω，驱动等级 0–3 对应 180/90/60/45 Ω；上拉 Rpu 经开关接本组的“VCC”，下拉 Rpd 经开关接 GND。图里的 VCC 指本组 IO 电源，没有画 VCC-PF 与 VCC-IO 之间的切换开关。datasheet 写每个 GPIO 最大驱动 6 mA，DC 参数 VIH 为 0.7 × 电源、VIL 为 0.3 × 电源、VOH 为电源 − 0.3 V。
- SDIO 电平：datasheet 表 5-5/5-6 给出 3.3 V 档 VCCQ 为 2.7–3.6 V，1.8 V 档 VCCQ 为 1.7–1.95 V，两档卡 VDD 都是 2.7–3.6 V。

若 0x0350 按上面推断工作，现版上电默认 3.3 V 档，BootROM 与普通 SD 初始化按 3.3 V 信号工作，SMHC0 最高 HS 50 MHz。UHS（SDR50/SDR104，手册写 SDR 150 MHz@1.8 V）需按 SD 3.0 流程切换：ACMD41 带 S18R，卡回 S18A 后发 CMD11（SMHC 命令寄存器 VOL_SW 位），停卡时钟，把 0x0350 bit0 清 0、0x0340 bit5 置 1，等至少 5 ms 再恢复时钟。主线 6.18 的 sunxi pinctrl 只按 `vcc-pf-supply` 写 0x0340/0x0344，sunxi-mmc 不写 0x0350，要用 UHS 需自写一个把 0x0350 当作 vqmmc 的 1.8/3.3 V 稳压器驱动。新版 CARD1 的 VDD 已接 3.3 V。芯片没有 PF 电源切换后的输出引脚（datasheet 电源脚只有 LDOA/LDOB 等是输出），外部上拉接哪个轨都有一档不对：接 3.3 V 在 1.8 V 档倒灌，接 1.8 V 在 3.3 V 档给出的高电平低于卡的 VIH。片内上拉随 PF 域切换：PF3（CMD）和 PF6 为 15 kΩ（12–18），其余 PF 为 100 kΩ（80–120），见 datasheet 表 5-4。SD 规范要求 CMD/DAT 上拉 10–100 kΩ，只用于空闲防浮空，数据传输时两端推挽驱动，速度由走线、串阻和 ESD 电容决定，与上拉无关。T113-i EVB 的 TF 卡 CMD 上拉 RC2、CD 上拉 RC3 都标 NC，DAT 线没有外部上拉，CLK 串 0 Ω、ESD 用 ESD9B5VL。硬件指南要求 CLK 串 33 Ω 靠 SoC，CMD 上 ESD 寄生电容不超过 10 pF。片内切换的实际电平需实板测 PF 脚高电平确认。核对 PF 信号电压和 CD 极性后再启用节点。

参考板都只跑 3.3 V 的 SD。T113-i EVB 有 TF 卡，VCC-PF 接 3.3 V。百问网 NezhaSTU 核心板把 VCC-PF 接 3.3 V，板载 SD NAND（MKDV2GCL-AB）挂在 PC 口的 SDC2，SDC0 引到邮票孔；DevKit 底板的 TF 卡 CMD/DAT 外部上拉 47 kΩ、CD 10 kΩ 到 VCC_3V3，CLK 串 33 Ω，只能用 3.3 V。邮票孔另有 1V8_OUT 脚。Tina 的 T113-i board.dts 中 SDC0 写 `max-frequency = <150000000>`、`cap-sd-highspeed`，三种 `sd-uhs-*` 全注释，实际停在 HS 50 MHz；DevKit 出厂 sys_config 的 card0 为 `card_high_speed = 1`。百问网 Tina 文档的 MMC 开发指南（适用列表为 A133、H616、V853、R853、F133）只列出 `sd-uhs-sdr50/ddr50/sdr104`、`vqmmc18sw/33sw-supply`（“按实际 PMU 供电方案修改”）和 `sunxi-dis-signal-vol-sw`（“关闭电压切换”），没有切换时序、无 PMU 方案或 VCC-PF 的说明。T113 的 SD UHS 没有可抄的公开参考设计。飞凌 OK113i-S（FET113i-S 核心板，T113-i）的产品页和规格书都写“SDC0 仅支持3.3V IO 电平，默认配置为SD 卡，SDC1 仅支持3.3V IO 电平”，没写 VCC-PF 和 SD 速率，代码资料需登录。[飞凌 OK113i-S](https://www.forlinx.com/product/t113-169.html) · [FET113i-S 规格书](https://www.forlinx.com/activity/FET113i-S_Product_spec.pdf)

Tina 的 PF 电压切换做在 pinctrl 里，MMC 驱动只切 pinctrl 状态：
- 板级 DTS（overlay `evb1_auto/linux-5.4/board.dts`，DevKit 出厂 sys_config 与之相同）给 SDC0 定义两组 PF0–PF5 引脚状态：`default` 的 `power-source = <3300>`，`mmc_1v8` 的 `power-source = <1800>`。SoC dtsi 的 `pinctrl-names` 为 `"default","mmc_1v8","sleep","uart_jtag"`。
- 厂商 sunxi-mmc 的 `sunxi_mmc_signal_voltage_switch()`：有 `sunxi-dis-signal-vol-sw` 就直接返回；内核开了 CONFIG_REGULATOR 时 `sunxi-signal-vol-sw-without-pmu` 被忽略、视为真。切 1.8 V 时先对 vqmmc 设 1.8 V（没有就跳过），再选 `mmc_1v8` 状态；切 3.3 V 时选 `default`。CMD11 由命令寄存器 VOL_SW 位发出。
- 厂商 pinctrl 只允许 PF 组用 `power-source`。5.4 版写 0x0350（3300 写 1、1800 写 0），并按对应稳压器设 0x0340 耐压档：1800 用 `vcc-pf-supply`，3300 用 `vcc-pfo-supply`（日志称 “bank PF optional regulator”），两个稳压器在 PF 引脚申请时都会被使能。5.15 BSP 的 `sunxi_power_switch_pf()` 升压时先把 0x0340 设 3.3 V 档、0x0344 PF 位写 1（代码注释为 “Disable self-adaption”）、0x0350 写 1，再轮询 0x0348 PF 位变 0；降压顺序相反。轮询失败直接 `panic("PF voltage switching failed")`。
- Tina SoC dtsi 中 `vcc-pf` 为 1.8 V、`vcc-pfo` 为 3.3 V 的固定稳压器；sun8iw20 的 pinctrl 描述设 `pf_power_source_switch = true`。PF 引脚初始化时跳过耐压设置（注释 “we don't know which voltage to use now”）。
- 所有板级 DTS 的 SDC0 只开 `cap-sd-highspeed`，`sd-uhs-*` 全部注释，`max-frequency` 是 150 MHz 上限；`sunxi-dis-signal-vol-sw` 也注释掉。MMC 核心只有主机声明 UHS 能力才在 ACMD41 带 S18R，所以电压切换代码虽然开着，实际不会触发，SD 停在 HS 50 MHz。
- 代码来源：[AvaotaSBC/linux linux-5.15 的 bsp/drivers](https://github.com/AvaotaSBC/linux/tree/linux-5.15/bsp/drivers)（`mmc/sunxi-mmc.c`、`pinctrl/pinctrl-sunxi.c`、`pinctrl/pinctrl-sun8iw20.c`），[orangepi-xunlong/linux-orangepi orange-pi-5.4-sun50iw9](https://github.com/orangepi-xunlong/linux-orangepi/tree/orange-pi-5.4-sun50iw9/drivers)（同一套厂商 5.4 驱动，`pinctrl/sunxi/pinctrl-sunxi.c`、`mmc/host/sunxi-mmc.c`）。2026-11-21 查阅，未存入仓库。

由此可得的推断（均无原厂文字确认）：
- PF 域在片内确有两路电源，由 0x0350 选择。厂商驱动把它建模为 `vcc-pf`（1.8 V 档）和 `vcc-pfo`（3.3 V 档）两个同时使能的电源，并用 0x0348 的电压检测确认切换完成。T113-i 只有 VCC-PF 一个 PF 电源球，所以另一路在片内，最可能是 VCC-IO。1.8 V 档对应 VCC-PF 球这一点有 5.4 代码的映射支持，比只看手册更可信。
- 原厂期望的 UHS 接法是 VCC-PF 球接 1.8 V、VCC-IO 接 3.3 V，两路同时上电，由片内开关选择。两路同时存在是设计工况，原 1V8 接法本身可能就是这种接法，倒灌风险低于此前估计，但仍无实测。
- 三块参考板 VCC-PF 都是 3.3 V（EVB、NezhaSTU 原理图；飞凌为“仅支持 3.3V IO”的推断）。此时两档都是 3.3 V，选 `mmc_1v8` 后 PF 仍是 3.3 V：5.15 BSP 会因 0x348 检测不到低压而 panic，5.4 会在 CMD11 后继续用 3.3 V 信号，卡初始化失败。这解释了所有参考 DTS 都关掉 UHS，以及飞凌写“仅支持 3.3V IO 电平”。
- 0x0344 的写法与手册不一致：手册说该位置 1 只用于 2.5 V，驱动切换 PF 时却总写 1 并称为关闭自适应。照抄时以驱动为准，实板再确认。

决定（2026-11-21）：VCC-PF（U18.F4）保留 1V8，按上面推断的原厂 UHS 接法，为以后 SD UHS 留余地。SD 先跑 HS 50 MHz（0x0350 复位为 3.3 V 档）。片内切换的拓扑无文档，若推断有误，3.3 V 可能倒灌进共用的 1V8 轨（包括 LT8619C 的 1.8 V 电源和 CN2 的 1.8 V 供电脚），首次上电先测 1V8 轨、PF 高电平和 0x0348；有问题时把 U18.F4 手工改接 3V3_MAIN。

补丁 0006 将 PF 片内选择器接为 MMC 的 `vqmmc`。DT 含 `pf-vqmmc` 时，pinctrl 跳过普通 PF 耐压设定，由这个调压器按 Allwinner BSP 顺序保护 pad 并更新 0x0350。通用 MMC 栈负责 CMD11 前后检查 DAT0 和控制卡时钟。[Tina MMC 开发指南](https://tina.100ask.net/SdkModule/Linux_MMC_DevelopmentGuide-02/)

主板电源键 PE13、复位键 PE3 和电源灯输入 PE12 都为高有效，分别对应主 PIO gpiochip 的 line offset 141、131、140。HDD 灯在 H1/H3 间直通。H7 实体键走 KEY_ADC。内核已启用 GPIO 字符设备及 libgpiod 1.x 所需的 v1 ABI。

THS 位于 0x02009400，GIC SPI 58，校准读取 SID 偏移 0x14 的 8 字节。以 thermal_zone 暴露 SoC 温度，100 °C 临界阈值取自公开的 T113 DTS 提案。系统 WDT @0x020500a0 用于当前 ARMv7 FEL/U-Boot 组合；改用持有看门狗的固件时应保留 reserved。启动后检查校准、温度变化及 watchdog 身份；超时复位须单独安排实板验证。[THS 节点与校准来源](https://lists.openwall.net/linux-kernel/2026/01/13/2046)

## 参考

- https://github.com/gregkh/linux/tree/v6.18.64
- https://github.com/DongshanPI/T113i_DevKitF_Tina5SDK
- https://github.com/rockchip-linux/kernel/blob/develop-6.1/drivers/media/i2c/lt8619c.c
- https://forum.armbian.com/topic/26551-video-encoder-using-mainline-kernel/
