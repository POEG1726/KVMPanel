# KVMPanel 软件交接

最近五项有实际进展的软件工作，最新一项在顶部。全项目状态见 [软件进度](docs/software-progress.md)。

## T113 硬件 MJPEG

核对日期 2026-11-23。

- 已移植 Cedrus JPEG 编码和 T113 IOMMU，`kvmd` 自动发现 `cedrus-enc`，缺失或编码失败时回退 libjpeg-turbo。已构建 ARM 内核、组合 DTB 和静态应用，核对 DT 绑定及 NV12 填充。 [编码说明](docs/linux-6.18.md#jpegmjpeg-编码)
- 测试镜像为 `build/debian/kvmpanel-debian13-armhf-mjpeg.img.xz`。已检查分区和文件系统。QEMU 用 virt 内核验证了登录、KVM 健康接口、USB gadget 和模拟 STC 关机。
- 接手上板先确认 JPEG 输出、颜色和 VE 完成中断，再测 LT8619C → CSI → KVM 网页的帧率和 CPU 实占。用 `KVMD_MJPEG_DEVICE=off` 对照软件编码。

## Windows PC 遥测采集计划

核对日期 2026-11-23。

- 已核对截图与现有契约。清单纳入截图的非风扇传感器及每个 Windows 网络适配器。设备和指标动态发现，Panel 自行绑定显示内容。 [采集计划](docs/superpowers/specs/2026-11-23-pc-telemetry-design.md)
- 接手先做只读探测，在截图机器和不同硬件主机核对来源覆盖，再实现设备清单、Mock 和正式 HID 批传。真实采集及 USB 吞吐待验证。 [现有接口](software/app/ipkvm/README.md#副屏与遥测)

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

## 板级网口、显示电源与 HDMI 采集

核对日期 2026-11-23。

- RTL8211F-CG 的设备树已启用 EMAC，改用本地无源 25 MHz 晶体，配置 PC1 复位和 PC7 中断。SGM3851A 新增 `voltages_uv`，按手册编码并保留默认上电。LT8619C 通过 PC3 复位，使用 GPIO15/PE17 上报视频时序变化，通过 GPIO13 查询 HDMI 5 V。2026-11-22 版 microSD UHS 适配已一起构建。 [板级配置与操作](docs/linux-6.18.md)
- 已构建 Linux 6.18.64 ARM `zImage`、基础 DTB、DSI DTB 和组合 DTB，另行编译并链接了 LT8619C 与 SGM3851A 的 ARM 模块。组合 DTB 通过相关绑定检查。SGM3851A 编码测试覆盖表 1 的 53 个电压命令。构建输出位于 WSL 的 `~/work/KVMPanel/build/linux-phy-display-integration/`。
- 接手先核对新版 PHY 晶体网表。从供电、UART0、FEL 和 DDR 开始实板验证，随后测 PHY 时钟、复位和链路，不接面板测 SWIRE 与输出电压，并用示波器和 V4L2 核对 IRQ、HDMI 5 V、EDID 和采集。CARD1 待测 PF 电压、UHS 调谐和冷启动。 [网口时序](docs/development.md#单独验证本板网口) · [首次上板](docs/development.md#首次上板) · [LT8619C 步骤](docs/linux-6.18.md#lt8619c-和并行-csi)
