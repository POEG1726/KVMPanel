# Windows PC 遥测采集计划

目标是在 Windows 主机上发现设备，尽量采集该硬件实际公开的读数。Panel 根据设备和指标 ID 自行选取内容、顺序、单位和布局。切换页面、隐藏指标或者更换显示模板都不会改变 PC 采集范围。风扇转速、占空比、状态和控制数据均排除在本版遥测之外。

2026-11-23 的截图是 Ryzen 9 9950X3D、DDR5 和 NVIDIA 平台的首份核对清单。核心、CCD、GPU、显存芯片、内存槽、磁盘和网卡均由运行时发现。设计覆盖 Intel、AMD CPU，Intel、AMD、NVIDIA 显卡，以及核显与独显并存的机器。设备有多少就发现多少，装有不同型号或缺少某个传感器时仍可工作。

## CPU、内存与主板

| 分组 | 目标字段 | 采集粒度 |
| --- | --- | --- |
| CPU VID | Core VIDs、各核心 VID，单位 V | 每个 CPU、每个物理核心。VID 与实测 Vcore 各有独立字段 |
| CPU 时钟 | Core Clocks、Core Effective Clocks、Average Effective Clock、Bus Clock、Core Ratios | 分别采即时频率、有效频率、平均有效频率、总线基准频率、倍率。保留来源提供的汇总和核心性能顺位，顺位用作元数据 |
| CPU 负载 | Core Usage、Max CPU/Thread Usage、Total CPU Usage、Core Utility、Total CPU Utility、Average Active Core Count | 按来源实际提供的核心或逻辑线程采集，汇总单列。Utility 保留超过 100% 的读数，活跃核心数允许小数 |
| CPU 温度 | Tctl/Tdie、CPU Die Average、各 CCD Tdie、各核心温度、L3 缓存温度、IOD Hotspot | 保留来源提供的汇总和每个 CCD、核心、L3 实例的温度。通过拓扑说明核心归属 |
| CPU VRM 温度与实测电压 | VDDCR_VDD、VDDCR_SOC、VDD_MISC 各自的 VRM 温度和 SVI3 TFN 电压 | 每路保留温度和电压两项。其他平台发现来源实际公开的电源轨 |
| CPU 电流 | SVI3 TFN 的 Core、SoC、MISC Current、CPU TDC、CPU EDC | 单位 A。TDC/EDC 读数与对应限值百分比分开 |
| CPU 功耗 | CPU Package Power、Core Powers 汇总和各核心功耗、SVI3 TFN 的 CPU Core Power 和 CPU SoC Power、Core+SoC+MISC Power、CPU PPT | 单位 W。保留各自测量范围与来源 |
| CPU 互连和缓存时钟 | FCLK、UCLK、L3 Clocks 汇总和各 CCD 的 L3 时钟、Frequency Limit – Global | FCLK 是 Infinity Fabric 时钟，UCLK 是内存控制器时钟，另采内存 MCLK |
| CPU 限值 | CPU PPT Limit、CPU TDC Limit、CPU EDC Limit、CPU PPT FAST Limit、Thermal Limit | 保留来源报告的百分比 |
| CPU 限频状态 | Thermal Throttling HTC、PROCHOT CPU、PROCHOT EXT | 三路独立的布尔状态。也为其他平台公开的限频原因保留字段 |
| 内存带宽与用量 | DRAM 读取和写入带宽、物理内存总量、可用量、已用量、MCLK | 分开记录读写。兼容现有内存用量模板 |
| 每条 DDR5 DIMM | SPD Hub Temperature、VDD (SWA)、VDDQ (SWB)、VPP (SWC)、1.8 V VOUT、1.0 V VOUT、VIN、Total Power、PMIC High Temperature、PMIC Over Voltage、PMIC Under Voltage | 每个已安装槽位分别上报温度、六路电压、功耗和三个布尔状态。用 SMBIOS 槽位信息标识 DIMM，SPD/PMIC 读数由传感器来源提供 |
| 芯片组与主板 | Chipset 1 (xHCI) 温度、主板公开的其他温度/电压/功耗 | 按实际控制器或传感器实例发现。截图中的 xHCI 项需要实机核对来源 |
| 存储与整机 | 每个 SSD/NVMe 的温度与可用 SMART 信息，磁盘读写速率、容量，现有整机功耗模板所需读数 | 磁盘从设备清单发现。整机功耗仅在有实际测量来源时上报 |

Intel 及其他 CPU 平台复用相同语义的通用温度、时钟、负载、功耗等字段，能明确区分性能核和能效核时附上核心类型。厂商特有的功率或限频字段按来源扩展，保留来源定义、单位和范围。

## GPU

每块 GPU 单独上报。用设备实例标识区分同型号显卡，保留集成或独立类型。在来源支持时采集以下各项。

| 分组 | 目标字段 |
| --- | --- |
| 温度 | GPU Temperature、GPU Memory Junction、GPU Hot Spot、GPU Memory Chip Temperatures 汇总及各可读显存芯片温度 |
| 电压 | GPU Core Voltage、PCIe +12V Input、12VHPWR、MSVDD |
| 整卡功耗 | GPU Power、GPU Power Limit (rated)、GPU Power Limit (max) |
| 电源轨功耗 | PCIe +12V Input、12VHPWR、Unknown Rail (D6) Input (sum)、Core (NVVDD) Input (sum)、Unknown Rail (D4) (sum)、Misc0 Input (sum)、Unknown Rail (D) (sum)、MSVDD、Core (NVVDD) Output |
| 时钟、负载与显存 | GPU Clock，来源可读的显存时钟、GPU 利用率、专用或共享显存容量和占用。来源提供不同引擎负载时分项保留 |

截图中重复出现的 GPU 页按传感器 ID 合并。两行同名的 NVVDD Output 若有不同来源 ID，保留两条。未知轨保留原名和来源 ID。输入、输出、整卡值分别上报。

## 每个 Windows 网络适配器

发现物理以太网、Wi-Fi、USB 网卡、VPN、虚拟交换机和其它 Windows 可枚举接口，保留断开或停用的适配器状态。用 Windows 接口 GUID 作为主机内的接口标识，名称和当前索引作为属性。发现列表包含每个独立接口，Panel 可按物理、虚拟、活动状态筛选。

| 分组 | 每个适配器的字段 |
| --- | --- |
| 标识与链路 | 接口名称、描述、类型、物理或虚拟属性、管理状态、运行状态、媒体连接状态、MTU、接收和发送协商速率 |
| 实时流量 | 各自的接收和发送速率，单位 B/s。按连续两次八位组计数的差值和单调时钟间隔计算。初次读取、计数器重置、接口重连或采样失败后先建立新基线 |
| 计数器 | 累计收发字节，单播与非单播包，收发错误和丢弃计数，均保留方向和 64 位整数精度 |
| 可选接口信息 | IPv4/IPv6 地址与前缀、默认路由归属、可用时的 Wi-Fi 信号。地址信息默认留在 PC，按需启用上报 |

网络基础数据用 Windows IP Helper 的接口表。地址和 Wi-Fi 信息分别在可用时通过系统接口获取。系统流量卡片可绑定用户选的实际适配器，避免把 VPN、虚拟交换机及承载它们的物理网卡自动相加。 [GetIfTable2](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/nf-netioapi-getiftable2) · [MIB_IF_ROW2](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/mib-if-row2) · [GetAdaptersAddresses](https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getadaptersaddresses) · [WlanQueryInterface](https://learn.microsoft.com/en-us/windows/win32/api/wlanapi/nf-wlanapi-wlanqueryinterface)

## 发现、兼容与采样

- **发现和标识**。上报主机内稳定的设备 ID、设备类别、厂商型号、拓扑或父设备，以及可读指标的语义 ID、原始名称、来源、单位和数据类型。实际键由设备 ID 加指标 ID 组成。设备或来源标签变化时对齐原有语义。遇到多路同名读数，用来源的原生 ID 区分。断线重连后刷新完整清单。
- **来源选择**。Windows 系统接口提供网络、容量和设备信息。硬件传感器先验证 LibreHardwareMonitor 的跨厂商覆盖，再按需使用 GPU 厂商公开接口。NVML 是 NVIDIA GPU 的候选补充。逐项记录可读、未公开、权限受限或读取失败。截图里的 SVI3、显存芯片、GPU 电源轨和 DDR5 PMIC 遥测需要按机型核对。 [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) · [NVIDIA NVML](https://docs.nvidia.com/deploy/nvml-api/latest/api/group__nvmlFieldValueEnums.html) · [Windows 物理内存设备](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-physicalmemory)
- **视图兼容**。保留已有 `cpu.tctl`、`gpu.temp`、`mem.dimm.<槽位>` 等模板可用的映射和单位。相同语义且来源证实等价时才建立映射。网络模板的 `net.rx` 和 `net.tx` 可绑定一张适配器。新布局可直接引用实例指标，也可按设备类别和指标语义自动发现。
- **数值和缺样**。统一新指标的单位，例如 MHz、°C、V、A、W、B/s 和 b/s。标量、布尔与 64 位累计计数按类型传输。不存在的指标从清单省略。已发现但当次读不到时传 null。断线或过期以时间戳标识。首次速率基线尚未建立时传 null，真实的零和 false 正常上报。
- **采样与统计**。初定负载、时钟、带宽、功耗、温度和网卡流量每秒采一次，慢变或读取成本较高的来源按实测采用 2–5 秒周期。按指标记录采样时刻。PC 或 Panel 可用有效历史记录计算选定窗口的最小值、最大值、平均值。现有契约 `min/max` 是显示量程，历史统计单独表达。
- **USB 传输**。现有临时 HID 每个 64 字节报告只能更新一项。正式协议先在同一 HID 功能上验证带版本、序号、会话与短指标 ID 的批量编码，设备清单独立分段。用序号识别丢包并标为缺样，连接恢复后重发清单。按实际 USB 吞吐确定批大小、快慢指标周期和容量上限。 [现有临时 HID](../../../software/app/ipkvm/README.md#副屏与遥测)

## 实施顺序

1. 做 Windows 只读探测器，导出实际设备、来源、字段覆盖和代表样本。先核对截图机器，再在不同 CPU/GPU 组合及多网卡主机上验证。准备多 GPU、不同核心数、不同 DIMM 布局、缺传感器、同名传感器和网卡热插拔的回归样本。
2. 将设备清单、指标映射、类型与缺样规则落到契约和 Mock，验证现有布局映射和 Panel 任意按实例选取。检查整个采集输出不包含风扇数据。
3. 实现 Windows 采集器、正式 HID 批传和 `kvmd` 接收。用不同字段数量和采样频率测试丢包恢复、连接重建、64 位计数、多个适配器独立流量以及 CPU 开销。
4. 实板跑 Windows 至 Panel 全链路，记录 USB 峰值报告率、采样延迟、丢样和长期运行行为。Panel 用同一份设备清单验证跨主机布局。
