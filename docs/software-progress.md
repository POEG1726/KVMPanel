# KVMPanel 软件进度

最近更新 2026-11-23。实板验证以实际测试记录为准。最近五项软件工作的接手入口见 [HANDOFF.md](../HANDOFF.md)。

## 已完成

- **接口与 Mock**。OpenAPI、WebSocket、panel socket 和本机 Mock 已合入 `main`。Mock 源码和测试已涵盖关闭绝对鼠标时遥测占用独立 IN 端点的规则。 [接口与运行说明](../software/app/api/README.md)
- **Linux 基础系统与板级驱动的离线验证**。Linux 6.18.64、板级 DTB、FEL 用 U-Boot 和 Buildroot initramfs 已有构建结果。已有 QEMU 用户态、USB gadget 及 LT8619C、CSIC、TD4322 模块装卸记录，以及 C906/HiFi4 回环固件构建和打包记录。 [构建说明](development.md) · [板级验证范围](linux-6.18.md)
- **PHY、SGM3851A 与 LT8619C 软件**。RTL8211F-CG 启用本地无源晶体方案、PC1 复位和 PC7 中断。SGM3851A 加入有符号微伏接口，编码测试覆盖手册全部 53 个电压命令。LT8619C 加入 PC3 复位、GPIO15 中断和 GPIO13 的 HDMI 5 V 查询。ARM 内核和三份板级 DTB 已构建，相关 DT 绑定检查通过。SGM3851A 与 LT8619C 也分别通过模块编译和链接。 [驱动验证范围](linux-6.18.md)
- **KVM 网页**。登录、远程控制台、设置、文件和终端工作台已合入 `main`。已有 Chrome 对 Mock 的双语走查，以及 `kvmd --sim` 静态页面、路由和文件往返验证。 [前端验证记录](../software/app/web/README.md)
- **副屏原型**。LVGL 9、Slint、Qt 6 Quick 的竖屏原型分别在 `feat/panel-proto`、`feat/panel-slint`、`feat/panel-qt`，各带 PC 端帧率和 CPU 记录。 [原型计划](superpowers/plans/2026-11-20-04-panel-prototypes.md)
- **`kvmd` 后端**。模拟模式、板端服务、Buildroot 打包和新版应用启动失败后的回滚逻辑已实现。已有 ARM 镜像在 QEMU 的 `/api/health` 与 dummy_hcd 冒烟记录。 [后端说明](../software/app/ipkvm/README.md) · [镜像验证](development.md#构建-kvmd-与验证镜像)

## In Progress

- **板级适配**。2026-11-22 归档与软件已用于 microSD UHS、显示电源和采集适配。待新版网表核对 RTL8211F-CG 的无源晶体接法。待实板验证 PHY 起振和链路、SGM3851A 的 SWIRE 波形和三路输出、LT8619C 的 IRQ 清除及 HDMI 采集，以及 microSD 的电压和速率。 [板级适配](linux-6.18.md) · [网口时序](development.md#单独验证本板网口)
- **STC8 电源管理集成**。固件 1.1 加入 STC 本机硬件看门狗，已通过 SDCC 构建、状态机测试及模拟 ISP 回归。引脚分配 CSV 已记录按键理论电压。实板待验证 ADC 判定窗口、UART 电平和收发、供电与复位波形、看门狗恢复，以及外部 UPDATE_HOLD 电路。 [STC8 说明与验证](../software/firmware/stc8/README.md)

## TODO

- **基础系统实板 bring-up**。按授权范围先核对供电和 UART0，再用 FEL 进 RAM 验证 DDR 训练、容量和 Linux 启动。随后验证 USB、网口、microSD 读写与冷启动。新版板子已移除 SPI NAND。 [首次上板](development.md#首次上板) · [当前硬件](../hardware/README.md)
- **板端功能联调**。基础系统跑通后验证 LT8619C/CSI、MJPEG 吞吐、USB 键鼠与存储、Windows 遥测、C906/HiFi4 RPMsg、按键、光照、温度和看门狗。确认实际面板与触摸型号后验证 DSI、供电、背光和触摸。 [板级适配清单](linux-6.18.md#结果)
- **副屏方向、离线状态页与正式实现**。先把共享 `panel.orientation` 和离线状态所需数据写进契约，再同步 Mock、`kvmd`、网页设置和副屏。对比三版原型后选定框架，补横屏、触摸坐标转换和 T113 实测。 [当前应用设计](superpowers/specs/2026-11-20-kvmpanel-app-design.md)
- **电脑端遥测**。真实上位机与板端线格式待定。现有 Mock、`kvmd --sim` 和临时 HID 解码可用于前期联调。 [遥测入口](../software/app/ipkvm/README.md#副屏与遥测)
