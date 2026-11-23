# KVMPanel 软件进度

最近更新 2026-11-23。实板验证以实际测试记录为准。最近五项软件工作的接手入口见 [HANDOFF.md](../HANDOFF.md)。

## 已完成

- **副屏电脑离线页（原型）**。panel socket 新增每秒一次的 `board` 消息和 `pc_power_on` 动作（电脑开着时拒绝），Mock 可从关机或等待上位机状态启动。Qt 6 和 LVGL 原型都实现了竖屏和横屏的离线页、开机按钮和息屏倒计时，曲线和内存温度行也按已定规则改过。已通过 Mock 测试和两版构建，并在 Mock 下截图核对关机、等待上位机、开机后切回主屏三种状态。实际点击和按键未在窗口里测过，`kvmd` 还不发 `board`。 [Dashboard](../software/app/dashboard/README.md) · [接口](../software/app/api/README.md)
- **T113 硬件 MJPEG 接入**。组合 DTB 启用 VE 和 IOMMU，`kvmd` 使用 Cedrus JPEG 编码并提供软件回退。ARM 内核、DTB、静态应用已构建，VE 和 IOMMU 绑定通过检查。已打包 Debian 镜像并通过 QEMU 用户态冒烟。 [编码说明](linux-6.18.md#jpegmjpeg-编码)
- **Windows PC 遥测采集计划**。截图中的非风扇指标已逐组纳入清单，补充了每块网络适配器，并定义跨硬件发现、采样状态与 Panel 的显示选择。已核对截图和现有契约。 [采集计划](superpowers/specs/2026-11-23-pc-telemetry-design.md)
- **电脑电源灯状态**。板端 `kvmd` 只读 PE12，REST 和 WebSocket 电源状态只包含 `on`。Mock、类型和网页均已同步。通过 `kvmd` 和 Mock 测试、网页构建及测试，实板电源灯读数待测。 [主板控制接线](../software/app/ipkvm/README.md#板端接口)
- **接口与 Mock**。OpenAPI、WebSocket、panel socket 和本机 Mock 已合入 `main`。Mock 源码和测试已涵盖关闭绝对鼠标时遥测占用独立 IN 端点的规则。 [接口与运行说明](../software/app/api/README.md)
- **Linux 基础系统与板级驱动的离线验证**。Linux 6.18.64、板级 DTB、FEL 用 U-Boot 和 Buildroot initramfs 已有构建结果。已有 QEMU 用户态、USB gadget 及 LT8619C、CSIC、TD4322 模块装卸记录，以及 C906/HiFi4 回环固件构建和打包记录。 [构建说明](development.md) · [板级验证范围](linux-6.18.md)
- **Debian 13 armhf SD 镜像**。已构建板用镜像并预装 Vim。QEMU 验证首次密码设置、数据分区、KVM 网页、USB gadget、网络设置及模拟 STC8 关机。已测启动和空闲峰值，并对照旧 Buildroot，实板负载待测。 [镜像说明](../software/os/debian/README.md) · [内存对照](../software/os/buildroot/README.md#qemu-内存占用)
- **PHY、SGM3851A 与 LT8619C 软件**。RTL8211F-CG 启用本地无源晶体方案、PC1 复位和 PC7 中断。SGM3851A 加入有符号微伏接口，编码测试覆盖手册全部 53 个电压命令。LT8619C 加入 PC3 复位、GPIO15 中断和 GPIO13 的 HDMI 5 V 查询。ARM 内核和三份板级 DTB 已构建，相关 DT 绑定检查通过。SGM3851A 与 LT8619C 也分别通过模块编译和链接。 [驱动验证范围](linux-6.18.md)
- **KVM 网页**。登录、远程控制台、设置、文件和终端工作台已合入 `main`。已有 Chrome 对 Mock 的双语走查，以及 `kvmd --sim` 静态页面、路由和文件往返验证。 [前端验证记录](../software/app/web/README.md)
- **副屏原型**。LVGL 9、Slint、Qt 6 Quick 的竖屏原型分别在 `feat/panel-proto`、`feat/panel-slint`、`feat/panel-qt`，各带 PC 端帧率和 CPU 记录。 [原型计划](superpowers/plans/2026-11-20-04-panel-prototypes.md)
- **`kvmd` 后端**。模拟模式、板端服务、Buildroot 打包和新版应用启动失败后的回滚逻辑已实现。已有 ARM 镜像在 QEMU 的 `/api/health` 与 dummy_hcd 冒烟记录。 [后端说明](../software/app/ipkvm/README.md) · [镜像验证](development.md#构建-kvmd-与验证镜像)

## In Progress

- **板级适配**。2026-11-22 归档与软件已用于 microSD UHS、显示电源和采集适配。待新版网表核对 RTL8211F-CG 的无源晶体接法。待实板验证 PHY 起振和链路、SGM3851A 的 SWIRE 波形和三路输出、LT8619C 的 IRQ 清除及 HDMI 采集，以及 microSD 的电压和速率。 [板级适配](linux-6.18.md) · [网口时序](development.md#单独验证本板网口)
- **STC8 电源管理集成**。固件 1.1 加入 STC 本机硬件看门狗，已通过 SDCC 构建、状态机测试及模拟 ISP 回归。引脚分配 CSV 已记录按键理论电压。实板待验证 ADC 判定窗口、UART 电平和收发、供电与复位波形、看门狗恢复，以及外部 UPDATE_HOLD 电路。 [STC8 说明与验证](../software/firmware/stc8/README.md)

## TODO

- **Windows PC 遥测实现**。先探测不同主机的传感器覆盖，再实现设备清单契约、Windows 采集器、正式 HID 批传和 Panel 选择。现有 Mock 和临时 HID 可用于联调。 [实施顺序](superpowers/specs/2026-11-23-pc-telemetry-design.md#实施顺序)
- **基础系统实板 bring-up**。按授权范围先核对供电和 UART0，再用 FEL 进 RAM 验证 DDR 训练、容量和 Linux 启动。随后验证 USB、网口、Debian SD High Speed 冷启动，以及实板断电后的文件系统。完成 3.3 V 读写测试后，再验证 UHS。 [首次上板](development.md#首次上板) · [写卡与启动](../software/os/debian/README.md)
- **板端功能联调**。基础系统跑通后验证 LT8619C/CSI、VE 真实 JPEG 帧及 MJPEG 吞吐、USB 键鼠与存储、Windows 遥测、C906/HiFi4 RPMsg、按键、光照、温度和看门狗。确认实际面板与触摸型号后验证 DSI、供电、背光和触摸。 [板级适配清单](linux-6.18.md#结果)
- **副屏方向与正式实现**。离线页已在 Qt 和 LVGL 原型中完成。还需让 `kvmd` 发送 `board` 并处理 `pc_power_on`，补 `panel.orientation` 契约和网页设置，实现横屏主屏方案 B 和触摸坐标转换，选定框架后在 T113 上实测。原型在 `feat/panel-qt` 和 `feat/panel-proto`，尚未合入 `main`。 [当前应用设计](superpowers/specs/2026-11-20-kvmpanel-app-design.md) · [Dashboard](../software/app/dashboard/README.md) [当前应用设计](superpowers/specs/2026-11-20-kvmpanel-app-design.md)
