# KVMPanel 软件交接

最近五项有实际进展的软件工作，最新一项在顶部。全项目状态见 [软件进度](docs/software-progress.md)。

## Windows PC 遥测采集器

核对日期 2026-11-23。

- 原生 C/Win32 采集器带设置窗口和托盘。Windows x64 构建通过，本机验证 CPU、内存、磁盘、逐网卡和 GPU 基础指标。RTX 5090 管理员探测同时取得核心温度 29 °C、核心热点 31.1875 °C 和显存结温 40 °C。普通权限访问 PawnIO 被拒时仍可采集核心温度及显存结温。 [采集器与只读探测](software/app/pc-telemetry/README.md)
- `kvmd` 已接收批量 HID 遥测、设备名和会话心跳，离线测试和 ARM 静态构建通过。接手用 `--once` 核对其他显卡，再测实板 USB 收发并接入正式 Dashboard 的字段选择。CPU 超频与供电读数、DRAM 带宽、DIMM 温度和 PMIC 仍需硬件采集实现。 [采集计划](docs/superpowers/specs/2026-11-23-pc-telemetry-design.md) · [板端协议](software/app/pc-telemetry/README.md#usb-与-panel)

## 副屏电脑离线页

核对日期 2026-11-23。

- panel socket 新增 `board`（板子自身状态，每秒一次）和 `pc_power_on`（电脑开着时回 `pc_on`）。Mock 已同步并有测试，`KVMPANEL_MOCK_PC=off|waiting` 可直接进入离线状态。 [接口说明](software/app/api/README.md)
- Qt 6（`feat/panel-qt`）和 LVGL（`feat/panel-proto`）原型都有竖屏和横屏的离线页、开机按钮和息屏倒计时。两版都已构建，并在 Mock 下截图核对关机、等待上位机、开机后切回主屏三种状态。窗口里的实际点击和按键未测。横屏主屏方案 B 未做，横屏时暂把竖屏主屏缩小居中。 [Dashboard](software/app/dashboard/README.md)
- 接手先让 `kvmd` 按契约发 `board` 并处理 `pc_power_on`（PE13 短按，PE12 亮时拒绝），再补 `panel.orientation` 和横屏主屏。两个原型分支尚未合入 `main`。

## T113 硬件 MJPEG

核对日期 2026-11-23。

- 已移植 Cedrus JPEG 编码和 T113 IOMMU，`kvmd` 自动发现 `cedrus-enc`，缺失或编码失败时回退 libjpeg-turbo。已构建 ARM 内核、组合 DTB 和静态应用，核对 DT 绑定及 NV12 填充。 [编码说明](docs/linux-6.18.md#jpegmjpeg-编码)
- 测试镜像为 `build/debian/kvmpanel-debian13-armhf-mjpeg.img.xz`。已检查分区和文件系统。QEMU 用 virt 内核验证了登录、KVM 健康接口、USB gadget 和模拟 STC 关机。
- 接手上板先确认 JPEG 输出、颜色和 VE 完成中断，再测 LT8619C → CSI → KVM 网页的帧率和 CPU 实占。用 `KVMD_MJPEG_DEVICE=off` 对照软件编码。

## 电脑电源灯状态

核对日期 2026-11-23。

- 板端只读 PE12。HTTP `/api/power` 和控制台 WebSocket 电源状态均只传 `on`，Mock、接口契约和网页已同步。 [板端接口](software/app/ipkvm/README.md#板端接口)
- 已通过 `kvmd` 和 Mock 测试、网页构建及测试。接手在实板分别核对开机和关机时的 PE12 读数及网页状态。

## Debian 13 SD 镜像

核对日期 2026-11-23。

- SD 系统采用 Debian 13 armhf 并预装 Vim，使用原板级内核及带 SD 节点的 U-Boot。默认 High Speed，另有 UHS 菜单项。压缩镜像位于本地忽略目录 `build/debian/kvmpanel-debian13-armhf.img.xz`。 [构建与写卡](software/os/debian/README.md)
- 离线通过分区、文件系统和 DTB 检查。QEMU 验证首次密码设置、Vim、数据分区、KVM 网页、USB gadget、网络配置、关机阻止锁及模拟 STC8 UART 的准备和待断电帧。QEMU 通过内核直启。
- 512 MiB QEMU 中，Debian 启动采样最高为 108.6 MiB，空闲 30 秒平均 70.7 MiB。已有配置时旧 Buildroot 的 initramfs 与 ext4 分别平均 71.3 和 50.1 MiB。实板 KVM 负载内存待测。 [内存对照](software/os/buildroot/README.md#qemu-内存占用)
- 上板从默认 High Speed 冷启动和串口登录开始，核对 SD 读写、以太网、KVM 外设和内核发出的 UART2 最终断电帧。FEL 与 Buildroot initramfs 可用于诊断。 [首次上板](docs/development.md#首次上板)
