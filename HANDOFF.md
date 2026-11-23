# KVMPanel 软件交接

最近五项有实际进展的软件工作，最新一项在顶部。全项目状态见 [软件进度](docs/software-progress.md)。

## Windows PC 遥测采集器

核对日期 2026-11-23。

- 原生 C/Win32 采集器带设置窗口和托盘。Windows x64 构建通过，本机验证 CPU、内存、磁盘、逐网卡和 GPU 基础指标。RTX 5090 管理员探测同时取得核心温度 29 °C、核心热点 31.1875 °C 和显存结温 40 °C。普通权限访问 PawnIO 被拒时仍可采集核心温度及显存结温。 [采集器与只读探测](software/app/pc-telemetry/README.md)
- 本机验证 Ryzen 时钟域、供电读数、倍频、功耗限值和两条 DDR5 的 SPD 温度、PMIC 告警、功率及六路电压，另已加锁核对 ADC 恢复。内部 PLL BCLK 实测 100 MHz，SMU DRAM 带宽已通过顺序读写负载区分两个方向。接手用 `--once` 核对其他主板、非默认 BCLK、9800X3D 和 5600X 的带宽，再测实板 USB。Intel GPU 深度读取待实现。 [CPU 和 DIMM 采集](software/app/pc-telemetry/README.md#cpu-和-dimm-硬件读数)

## 板端 Dashboard 与指标选择

核对日期 2026-11-23。

- `kvmd` 每秒上报 `board`，通过 `metric_catalogue` 提供包含隐藏项的清单，检查 PE12 后才处理 `pc_power_on`。板端 Dashboard 用 LVGL framebuffer 逐项显隐、四方向旋转，主机离线时显示板子状态并可息屏唤醒。Rust、Mock、网页、ARM 静态构建及原生和 ARM/QEMU 无屏冒烟均通过。 [Dashboard](software/app/dashboard/README.md) · [协议](software/app/api/README.md#副屏-panel-socket)
- `build/debian/kvmpanel-debian13-armhf-dashboard.img.xz` 包含副屏、`kvmd`、WS2812 和 MJPEG 内核，已离线检查文件系统、服务及组合 DTB。接手先核对实际屏、触摸设备和 /dev/fb0，再测试 USB 实板数据、开机按钮、息屏和方向。当前主屏用分组指标列表，原型的横屏方案 B 及曲线排版尚待移植。 [镜像构建](software/os/debian/README.md)

## WS2812 板级驱动

核对日期 2026-11-23。

- 组合 DTB 启用 T113 硬件 LEDC，以 PG13 驱动四颗 WS2812B-2020-V6。完成 ARM zImage 构建、驱动链接及设备树绑定检查。输出在 WSL 的 `~/work/KVMPanel/build/linux-ws2812/`。 [WS2812 接线、时序与 sysfs](docs/linux-6.18.md#ws2812)
- 实板可用后测 PG13 的位时序和复位低电平，逐颗核对 LED1、LED2、LED4、LED5 的三色及亮度。

## T113 硬件 MJPEG

核对日期 2026-11-23。

- 已移植 Cedrus JPEG 编码和 T113 IOMMU，`kvmd` 自动发现 `cedrus-enc`，缺失或编码失败时回退 libjpeg-turbo。已构建 ARM 内核、组合 DTB 和静态应用，核对 DT 绑定及 NV12 填充。 [编码说明](docs/linux-6.18.md#jpegmjpeg-编码)
- 测试镜像为 `build/debian/kvmpanel-debian13-armhf-mjpeg.img.xz`。已检查分区和文件系统。QEMU 用 virt 内核验证了登录、KVM 健康接口、USB gadget 和模拟 STC 关机。
- 接手上板先确认 JPEG 输出、颜色和 VE 完成中断，再测 LT8619C → CSI → KVM 网页的帧率和 CPU 实占。用 `KVMD_MJPEG_DEVICE=off` 对照软件编码。

## 电脑电源灯状态

核对日期 2026-11-23。

- 板端只读 PE12。HTTP `/api/power` 和控制台 WebSocket 电源状态均只传 `on`，Mock、接口契约和网页已同步。 [板端接口](software/app/ipkvm/README.md#板端接口)
- 已通过 `kvmd` 和 Mock 测试、网页构建及测试。接手在实板分别核对开机和关机时的 PE12 读数及网页状态。
