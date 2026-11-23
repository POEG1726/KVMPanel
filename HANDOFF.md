# KVMPanel 软件交接

最近五项有实际进展的软件工作，最新一项在顶部。全项目状态见 [软件进度](docs/software-progress.md)。

## MCP 服务

核对日期 2026-11-23。

- 现状：`kvmd` 只有 MCP 开关、单把 key 管理，以及 `/api/mcp/v1/screenshot`、`/api/mcp/v1/hid` 两个 Bearer REST 接口，还没有 MCP 协议服务。 [system.rs](software/app/ipkvm/src/system.rs)
- 设计已定稿，实现交给 Codex。范围：标准 Streamable HTTP 入口 `/api/mcp/v1`，多把分权限 key，观察、输入、电源、介质和主机隐私五类工具，开机守护，按 `config.host` 设置的键进 BIOS，副屏和 WS2812 提示，功能键急停，操作日志，以及经厂商 HID Input report 4 的主机端程序通道。OCR 和剪贴板默认关。 [MCP 设计](docs/superpowers/specs/2026-11-23-mcp-design.md)
- 尚无代码、构建或模拟结果。接手按设计第 3 节的契约顺序开工，验证按第 13 节分别记录。

## T113 接口驱动强度

核对日期 2026-11-23。

- SDC0 从 SD 启动的 SPL 起设 Level 3（典型 45 Ω），U-Boot 和 Linux 写入同一档。Linux 的 RGMII PG 引脚设 `drive-strength = <40>`。DDR3 颗粒用 `MR1=0x40` 配置 40 Ω 输出和 120 Ω RTT_NOM，T113 DDR PHY 读端 ODT 已开启。 [启动配置](software/os/u-boot/README.md) · [板级设备树](docs/linux-6.18.md#触摸光照sd-和-gpio)
- SD/FEL U-Boot 和三份 Linux 板级 DTB 已构建，核对了 SPL 写入 SDC0 档位与 MR1 的指令及 DTB 属性。产物在 WSL 的 `~/work/KVMPanel/build/` 下，目录为 `u-boot-kvmpanel-drive-sd`、`u-boot-kvmpanel-drive-fel` 和 `linux-kvmpanel-drive`。旧镜像使用旧设置。有板后先用 FEL 复测 DDR 训练、容量和读写，再测 SD 冷启动、UHS 和 RGMII 千兆收发。 [首次上板](docs/development.md#首次上板)
- 已反汇编 T113、V853 Tina SPL 和 T113-i DevKit 的 SD/NAND Boot0、FES1，确认 DDR3 参考 `0x7b7bfb` 的低 24 位直写 PHY `0x03103140`。已核对 H3 的六个半字节与三个 ZQDR 的对应关系，以及 T113-i 原厂 DX0、DX1 引脚分组。待同型号寄存器资料或实板电气测量确认两者的对应、输出驱动字段和 Ω 编码后，再选主控 48 Ω、40 Ω 或最强档。 [ZQ 查证与源码位置](software/os/vendor/tina/README.md#主控-zq-编码查证)
- R528 和 D1 的 Tina SPL 走相同 ZQCR 写入路径。D1-H 哪吒配置及 Centauri DDR3 Boot0 用 `0x7b7bfb`，D1s DDR2 配置用 `0x7b7bf9`。R528 V1.8、D1-H V1.0 公开了 240 Ω 参考电阻修调寄存器，ZQCR 引脚映射及档位待确认。 [R528 与 D1 资料](software/os/vendor/tina/README.md#r528-与-d1-的板级配置和寄存器)

## Windows PC 遥测采集器

核对日期 2026-11-23。

- 原生 C/Win32 采集器带设置窗口和托盘。Windows x64 构建通过，本机验证 CPU、内存、磁盘、逐网卡和 GPU 基础指标。RTX 5090 管理员探测同时取得核心温度 29 °C、核心热点 31.1875 °C 和显存结温 40 °C。普通权限访问 PawnIO 被拒时仍可采集核心温度及显存结温。 [采集器与只读探测](software/app/pc-telemetry/README.md)
- 本机验证 Ryzen 时钟域、供电读数、倍频、功耗限值和两条 DDR5 的 SPD 温度、PMIC 告警、功率及六路电压，另已加锁核对 ADC 恢复。内部 PLL BCLK 实测 100 MHz，SMU DRAM 带宽已通过顺序读写负载区分两个方向。接手用 `--once` 核对其他主板、非默认 BCLK、9800X3D 和 5600X 的带宽，再测实板 USB。Intel GPU 深度读取待实现。 [CPU 和 DIMM 采集](software/app/pc-telemetry/README.md#cpu-和-dimm-硬件读数)
- 已知指标缺测时通过 HID 上报 `null`，副屏保留指标并显示 `—`。本机 `--once` 验证 BCLK 和 DRAM 读写的数值与缺值，模拟验证 HID 编码、动态设备清理和 Panel 数值恢复。实板待测 USB 上报和副屏显示。 [USB 遥测](software/app/pc-telemetry/README.md#usb-与-panel)

## 板端 Dashboard 与指标选择

核对日期 2026-11-23。

- `kvmd` 每秒上报 `board`，通过 `metric_catalogue` 提供包含隐藏项的清单，检查 PE12 后才处理 `pc_power_on`。板端 Dashboard 用 LVGL framebuffer 逐项显隐、四方向旋转，主机离线时显示板子状态并可息屏唤醒。Rust、Mock、网页、ARM 静态构建及原生和 ARM/QEMU 无屏冒烟均通过。 [Dashboard](software/app/dashboard/README.md) · [协议](software/app/api/README.md#副屏-panel-socket)
- `build/debian/kvmpanel-debian13-armhf-dashboard.img.xz` 包含副屏、`kvmd`、WS2812 和 MJPEG 内核，已离线检查文件系统、服务及组合 DTB。接手先核对实际屏、触摸设备和 /dev/fb0，再测试 USB 实板数据、开机按钮、息屏和方向。当前主屏用分组指标列表，原型的横屏方案 B 及曲线排版尚待移植。 [镜像构建](software/os/debian/README.md)

## WS2812 板级驱动

核对日期 2026-11-23。

- 组合 DTB 启用 T113 硬件 LEDC，以 PG13 驱动四颗 WS2812B-2020-V6。完成 ARM zImage 构建、驱动链接及设备树绑定检查。输出在 WSL 的 `~/work/KVMPanel/build/linux-ws2812/`。 [WS2812 接线、时序与 sysfs](docs/linux-6.18.md#ws2812)
- 实板可用后测 PG13 的位时序和复位低电平，逐颗核对 LED1、LED2、LED4、LED5 的三色及亮度。
