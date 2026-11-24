# KVMPanel 软件交接

最近五项有实际进展的软件工作，最新一项在顶部。全项目状态见 [软件进度](docs/software-progress.md)。

## 机箱物理按键

核对日期 2026-11-23。

- **代码**。E12 阻值已写入引脚分配表。T113 从 GPADC0 的 IIO raw 节点以 20 ms 采样，自动校准并确认七个键态。满量程码 4095 确认为全松开并保留基准。FUNC 以 700 ms 分短长按，接入副屏、远程暂停和 MCP 急停。STC 1.3 的双键窗口为 163–210，全松开窗口为 313–1023，迟滞 6 计数。 [kvmd 入口](software/app/ipkvm/README.md#机箱物理按键) · [STC 入口](software/firmware/stc8/README.md)
- **构建**。STC 1.3 镜像占用程序 Flash 6035 B。ARM 静态 `kvmd` 去符号后为 13,136,808 B，与 `7dbc0d1` 相同。网页构建通过，复核了原生和 ARM 副屏目标。三份 DTB 构建记录对应首次接入。
- **模拟**。Rust 57 项单测和 54 项集成、HTTP 22 个 MCP 工具、Mock 81 项和网页 181 项通过。可写 ADC 文件覆盖 E12 校准、误差、组合、4095 截幅松键和物理急停。无屏副屏、STC 状态机和模拟 ISP 回归通过。
- **实板待测**。用 `KVMD_KEY_ADC_TRACE=1` 记录 E12 板上松开和各组合的原始读数与基准。复核全松开与 AVCC 的余量、两端判定窗口、STC 双键短按开机与强关、FUNC 短长按手感和急停。 [GPADC 与 H7 接线](docs/linux-6.18.md#c906hifi4-与-gpadc)

## MCP 服务

核对日期 2026-11-23。

- **代码**。`/api/mcp/v1` 和 22 个工具已落地，保留 Bearer REST 截图和 HID。分权限密钥、人工接管、电源与介质确认、网页和副屏停止、WS2812 提示及操作日志已接入。Windows 程序用 HID Input 4 和 Output 3 交换请求和回复，剪贴板默认关闭。 [API 契约](software/app/api/README.md#mcp) · [HID 协议](software/app/api/host-agent.md)
- **构建**。网页、原生和 ARM Dashboard、WSL ARM 静态 `kvmd`、Windows x64 构建通过。ARM `kvmd` 去符号后为 13,030,312 字节，较 MCP 前增加 944 KiB。 [板端构建与验证](software/app/ipkvm/README.md#mcp-服务) · [Windows 构建与模拟](software/app/pc-telemetry/README.md#mcp-主机端通道)
- **模拟**。Rust 102 项、Mock 79 项、网页 179 项通过。原生和 ARM Linux 来宾中的 `kvmd --sim` 经 HTTP 覆盖全部 22 个工具，包括可写介质确认和强关途中急停。MCP Inspector 验证了截图、点击、开机守护和进 BIOS。无屏副屏验证了横幅和停止，Unix panel socket 联调取消了正在长按的模拟按键。Windows 验证了 `info`、`clipboard`、`eject` 和默认关闭。
- **人工输入优先更新**。设置页和 `host_status.control` 提供优先开关及 1–60 秒交还等待，默认开启及两秒。原生 Rust 104 项、HTTP 全部 22 个工具、Mock 80 项、网页构建和 181 项测试通过。验证了默认拦截与到期放行、关闭后输入交错和急停。上面的 ARM、Windows、Inspector 和副屏记录适用于 MCP 首版。 [管理接口](software/app/api/README.md#mcp)
- **实板待测**。接手从 [设计 §13](docs/superpowers/specs/2026-11-23-mcp-design.md#13-验证) 开始，核对人工和 MCP 键鼠交错、USB 双向吞吐、真实 LUN 2 弹出、绝对坐标、GPIO 急停、BIOS 连按、启动时间线、WS2812 和实际触摸。物理 FUNC 急停已通过原生模拟，实板待测。

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
