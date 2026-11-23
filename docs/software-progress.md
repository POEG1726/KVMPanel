# KVMPanel 软件进度

最近更新 2026-11-23。实板验证以实际测试记录为准。最近五项软件工作的接手入口见 [HANDOFF.md](../HANDOFF.md)。

## 已完成

- **MCP 服务离线实现**。板端提供标准 Streamable HTTP、22 个工具、分权限密钥、审计和网页及副屏急停。Windows 上位机通过厂商 HID 提供窗口和显示器信息、受限剪贴板和共享盘安全弹出。Rust、Mock、网页测试与 Windows x64、WSL ARM 构建通过。全部工具通过原生及 ARM Linux 来宾 HTTP 模拟。MCP Inspector 和无屏副屏验证通过。 [板端验证](../software/app/ipkvm/README.md#mcp-服务) · [上位机验证](../software/app/pc-telemetry/README.md#mcp-主机端通道)
- **全志 DDR PHY ZQ 寄存器对照**。离线核对 R528、D1、V853 与 T113 Tina SPL 的 ZQ 写入，复核 D1 板级配置、Centauri Boot0 的 ZQ 参数和全志公开手册。已追出 H3 六个半字节到三个 ZQDR 的校准路径，并核对 T113-i 的 DX0/DX1 引脚和 240 Ω ZQ 参考。 [ZQ 查证](../software/os/vendor/tina/README.md#主控-zq-编码查证)
- **SDC0、RGMII 与 DDR3 颗粒驱动配置**。SD 启动的 SPL、U-Boot 和 Linux 将 SDC0 设为 Level 3（典型 45 Ω），RGMII 的 PG 引脚设为 `drive-strength = <40>`。SD/FEL SPL 启用主控读端 ODT，三星 DDR3 用 `MR1=0x40` 选 40 Ω 输出和 120 Ω RTT_NOM。SD/FEL U-Boot 与三份板级 DTB 构建通过，核对了 SPL 指令和 DTB 属性。实板待复测。 [启动配置](../software/os/u-boot/README.md) · [板级设备树](linux-6.18.md#触摸光照sd-和-gpio)
- **Windows 遥测缺值保留**。已知指标缺测时上报 `null`，副屏保留行并显示 `—`。Windows x64 构建、本机 BCLK 和 DRAM 有无读取组件的探测、HID 编码模拟及板端缺值恢复测试通过。实板 USB 待测。 [遥测协议](../software/app/pc-telemetry/README.md#usb-与-panel)
- **Windows Ryzen DRAM 带宽**。通过已有 SMU 表独立采集实际读写流量，Panel 使用 B/s。Windows x64 构建、板端 Panel 测试及 9950X3D 顺序读写负载验证通过，实板 USB、9800X3D 和 5600X 待复测。 [带宽来源与验证](../software/app/pc-telemetry/README.md#cpu-和-dimm-硬件读数)
- **Windows Ryzen BCLK**。从 FCH 内部 PLL 读取超频参考频率。Windows x64 构建及 9950X3D 实测通过，读到 100 MHz。板端 Panel 测试通过，实板 USB、其它主板和非默认频率待测。 [BCLK 采集](../software/app/pc-telemetry/README.md#cpu-和-dimm-硬件读数)
- **板端 Dashboard 与指标选择**。LVGL framebuffer 程序接入 `kvmd` 的 `board`、`telemetry` 和指标清单。主机离线时显示板子状态，支持 PE12 检查后的短按开机、四方向旋转、逐项显隐和息屏唤醒。通过 Rust、Mock、网页测试、ARM 静态构建和原生及 ARM/QEMU 无屏测试。Debian 镜像已加入 Dashboard 服务和 WS2812 内核。 [Dashboard](../software/app/dashboard/README.md) · [镜像](../software/os/debian/README.md)
- **Windows CPU 与 DIMM 硬件读数**。支持指定 Ryzen SMU 表的核心温度、时钟域、供电读数、功耗限值和物理核心倍频。DDR5 SPD5118 温度及支持的 PMIC 告警、功率和六路电压接入 SMBus。Windows x64 构建通过，9950X3D 与两条 DIMM 实测，并核对 PMIC ADC 恢复。 [采集器与支持范围](../software/app/pc-telemetry/README.md#cpu-和-dimm-硬件读数)
- **WS2812 板级驱动**。组合 DTB 接入 PG13 的硬件 LEDC 和四颗独立 RGB 灯，按 WS2812B-2020-V6 手册设定 GRB 和发送时序。通过 ARM zImage 构建、驱动链接和 LEDC 绑定检查。实板波形及点灯待测。 [WS2812](linux-6.18.md#ws2812)
- **Windows PC 遥测基础采集**。原生 C 设置窗口和托盘支持调整采样间隔及指标组。Windows x64 构建和本机只读探测通过，验证基础 CPU、内存、磁盘、逐网卡及 GPU 指标。RTX 5090 管理员探测实测核心热点 31.1875 °C、显存结温 40 °C。`kvmd` 批量 HID 遥测接收已通过离线测试和 ARM 静态构建。 [采集器与验证范围](../software/app/pc-telemetry/README.md)
- **T113 硬件 MJPEG 接入**。组合 DTB 启用 VE 和 IOMMU，`kvmd` 使用 Cedrus JPEG 编码并提供软件回退。ARM 内核、DTB、静态应用已构建，VE 和 IOMMU 绑定通过检查。已打包 Debian 镜像并通过 QEMU 用户态冒烟。 [编码说明](linux-6.18.md#jpegmjpeg-编码)
- **电脑电源灯状态**。板端 `kvmd` 只读 PE12，REST 和 WebSocket 电源状态只包含 `on`。Mock、类型和网页均已同步。通过 `kvmd` 和 Mock 测试、网页构建及测试，实板电源灯读数待测。 [主板控制接线](../software/app/ipkvm/README.md#板端接口)
- **接口与 Mock**。OpenAPI、WebSocket、panel socket 和本机 Mock 已合入 `main`。Mock 源码和测试已涵盖关闭绝对鼠标时遥测占用独立 IN 端点的规则。 [接口与运行说明](../software/app/api/README.md)
- **Linux 基础系统与板级驱动的离线验证**。Linux 6.18.64、板级 DTB、FEL 用 U-Boot 和 Buildroot initramfs 已有构建结果。已有 QEMU 用户态、USB gadget 及 LT8619C、CSIC、TD4322 模块装卸记录，以及 C906/HiFi4 回环固件构建和打包记录。 [构建说明](development.md) · [板级验证范围](linux-6.18.md)
- **Debian 13 armhf SD 镜像**。已构建板用镜像并预装 Vim。QEMU 验证首次密码设置、数据分区、KVM 网页、USB gadget、网络设置及模拟 STC8 关机。已测启动和空闲峰值，并对照旧 Buildroot，实板负载待测。 [镜像说明](../software/os/debian/README.md) · [内存对照](../software/os/buildroot/README.md#qemu-内存占用)
- **PHY、SGM3851A 与 LT8619C 软件**。RTL8211F-CG 启用本地无源晶体方案、PC1 复位和 PC7 中断。SGM3851A 加入有符号微伏接口，编码测试覆盖手册全部 53 个电压命令。LT8619C 加入 PC3 复位、GPIO15 中断和 GPIO13 的 HDMI 5 V 查询。ARM 内核和三份板级 DTB 已构建，相关 DT 绑定检查通过。SGM3851A 与 LT8619C 也分别通过模块编译和链接。 [驱动验证范围](linux-6.18.md)
- **KVM 网页**。登录、远程控制台、设置、文件和终端工作台已合入 `main`。已有 Chrome 对 Mock 的双语走查，以及 `kvmd --sim` 静态页面、路由和文件往返验证。 [前端验证记录](../software/app/web/README.md)
- **副屏原型**。LVGL 9、Slint、Qt 6 Quick 的竖屏原型分别在 `feat/panel-proto`、`feat/panel-slint`、`feat/panel-qt`，各带 PC 端帧率和 CPU 记录。 [原型计划](superpowers/plans/2026-11-20-04-panel-prototypes.md)
- **`kvmd` 后端**。模拟模式、板端服务、Buildroot 打包和新版应用启动失败后的回滚逻辑已实现。已有 ARM 镜像在 QEMU 的 `/api/health` 与 dummy_hcd 冒烟记录。 [后端说明](../software/app/ipkvm/README.md) · [镜像验证](development.md#构建-kvmd-与验证镜像)

## In Progress

- **Windows PC 遥测补充与实测**。Intel GPU 深度读数待接入。有其它 Ryzen 主机时，用 `--once` 交叉验证 BCLK 和 DRAM 带宽。有板子时验证 USB 与 Panel 显示。 [采集计划](superpowers/specs/2026-11-23-pc-telemetry-design.md)
- **板级适配**。2026-11-22 归档与软件已用于 microSD UHS、显示电源和采集适配。待新版网表核对 RTL8211F-CG 的无源晶体接法。待实板验证 PHY 起振和链路、SGM3851A 的 SWIRE 波形和三路输出、LT8619C 的 IRQ 清除及 HDMI 采集，以及 microSD 的电压和速率。 [板级适配](linux-6.18.md) · [网口时序](development.md#单独验证本板网口)
- **STC8 电源管理集成**。固件 1.1 加入 STC 本机硬件看门狗，已通过 SDCC 构建、状态机测试及模拟 ISP 回归。引脚分配 CSV 已记录按键理论电压。实板待验证 ADC 判定窗口、UART 电平和收发、供电与复位波形、看门狗恢复，以及外部 UPDATE_HOLD 电路。 [STC8 说明与验证](../software/firmware/stc8/README.md)

## TODO

- **MCP 实板验证**。待可用板卡和授权硬件操作时，测绝对鼠标与采集画面的映射、BIOS 连按与启动时间线、实板 HID 吞吐、Windows 真实 LUN 2 弹出、GPIO 脉冲急停、WS2812 和实际触摸停止。物理功能键按键读取和阈值标定仍归入板级联调。 [验证范围](superpowers/specs/2026-11-23-mcp-design.md#13-验证)
- **T113 DDR PHY 输出阻抗**。SD/FEL SPL 当前使用 DDR3 参考编码 `0x7b7bfb`。待同型号 PHY 字段表及输出阻抗表，或实板电气测量，确认各字节对应引脚及 48 Ω、40 Ω 或最强档后调整。 [ZQ 查证](../software/os/vendor/tina/README.md#主控-zq-编码查证)
- **基础系统实板 bring-up**。按授权范围先核对供电和 UART0，再用 FEL 进 RAM 验证 DDR 训练、容量和 Linux 启动。随后验证 USB、网口、Debian SD High Speed 冷启动，以及实板断电后的文件系统。完成 3.3 V 读写测试后，再验证 UHS。 [首次上板](development.md#首次上板) · [写卡与启动](../software/os/debian/README.md)
- **板端功能联调**。基础系统跑通后验证 LT8619C/CSI、VE 真实 JPEG 帧及 MJPEG 吞吐、USB 键鼠与存储、Windows 遥测、C906/HiFi4 RPMsg、按键（含把功能键接到 `function_key` 各动作，目前 kvmd 和副屏都未读取物理按键）、光照、温度和看门狗。确认实际面板与触摸型号后验证 DSI、供电、背光和触摸。 [板级适配清单](linux-6.18.md#结果)
- **副屏实板验证与排版**。确认面板、触摸设备和实际分辨率后，测四个方向的触摸坐标、内存峰值、息屏恢复、帧率和 CPU 占用。横屏方案 B 与原型曲线排版仍待移植。 [当前应用设计](superpowers/specs/2026-11-20-kvmpanel-app-design.md) · [Dashboard](../software/app/dashboard/README.md)
