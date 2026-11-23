# Windows PC 遥测采集计划

采集范围以副屏日常监控为准，重点是温度、负载、功耗、容量和实际吞吐。Panel 按设备和指标 ID 选取显示内容、顺序和单位，切换页面或隐藏指标不改变 PC 采样。风扇数据不采集。

截图机器用于首轮核对。设备按运行时发现，覆盖 Intel 和 AMD CPU、Intel/AMD/NVIDIA GPU，以及多显卡、多内存槽、多磁盘和多网卡主机。某项指标能否上报由实际硬件和来源决定。

## 采集范围

| 设备 | 采集项 |
| --- | --- |
| CPU | 总占用、各核心占用、CPU 当前频率、主温度、封装功耗。来源提供时补充温度限频状态 |
| 每块 GPU | 占用、核心频率、核心温度、可读的热点与显存温度、显存已用与总量、整卡功耗。核显的共享显存明确标识 |
| 内存 | 物理内存已用与总量、每条可读 DIMM 的 SPD 温度。内存规格和已配置的数据率随设备信息上报 |
| 每块磁盘 | 温度、实时读写速率。型号和容量随设备信息上报 |
| 主板和整机 | 有明确来源的主板温度、实际整机功耗。主板温度取一个可识别的代表读数 |
| 每个网卡 | 链路状态、协商速率、实时上行和下行。Wi-Fi 可额外上报信号强度 |

CPU 主温度映射为该平台公开的主 CPU/封装温度。CPU 当前频率使用来源报告的 CPU 频率汇总。只有逐核频率时，可以明示为「最高核心频率」并用独立指标 ID。核心占用保留用于定位单核满载。内存数据率以 MT/s 表示。GPU 功耗和整机功耗各自采用可验证的读数，缺少整机测量来源时省略整机功耗。

移除 Effective Clock、逐核频率、VID、电压、电流、倍频、FCLK/BCLK/UCLK、逐核与分路功耗、限值百分比，以及逐 CCD、缓存和显存芯片的详细温度。DDR5 保留 DIMM 温度，移除 PMIC 电压、功耗和告警。DRAM 带宽、详细 SMART、网卡包计数、错误与丢弃计数、IP 和路由退出本版范围。

## 每个 Windows 网络适配器

物理以太网、Wi-Fi、USB 网卡、VPN 和虚拟网卡分别列出，断开或停用时仍可查看设备。以 Windows 接口 GUID 作为主机内的标识，并上报名称、类型和物理或虚拟属性。协商速率和实际流量均区分接收与发送。Panel 可将流量卡片绑定某张网卡，避免把虚拟网卡和承载它的物理网卡重复相加。

PC 用连续两次 64 位字节计数的差值和单调时钟间隔计算 B/s，只传计算后的速率。首次读取、接口重连、计数器重置或采样失败时重建基线。 [GetIfTable2](https://learn.microsoft.com/en-us/windows/win32/api/netioapi/nf-netioapi-getiftable2) · [MIB_IF_ROW2](https://learn.microsoft.com/en-us/windows-hardware/drivers/network/mib-if-row2) · [WlanQueryInterface](https://learn.microsoft.com/en-us/windows/win32/api/wlanapi/nf-wlanapi-wlanqueryinterface)

## 发现、兼容与采样

- **发现和标识**。每个读数由主机内稳定的设备 ID 和指标语义 ID 组成，清单附型号、来源、单位和类型，按需附拓扑、核心类型或槽位。断线重连时刷新清单。
- **来源选择**。Windows 系统接口提供网络、容量和设备信息。跨厂商硬件传感器先核对 LibreHardwareMonitor 的覆盖，GPU 按需使用厂商公开接口。逐项记录可读、未公开、权限受限或读取失败。 [LibreHardwareMonitor](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor) · [NVIDIA NVML](https://docs.nvidia.com/deploy/nvml-api/latest/api/group__nvmlFieldValueEnums.html) · [Windows 内存设备](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-physicalmemory)
- **副屏迁移**。沿用语义相符的温度、利用率和显存字段。更新正式布局时，将 `cpu.clock_eff` 换成当前频率、`cpu.ppt` 换成封装功耗，删去 BCLK、FCLK、Vcore 和 MCLK/UCLK 数值行。同一次契约变更同步 Mock、`kvmd` 和副屏适配。网卡和磁盘按设备绑定。
- **采样**。负载、时钟、温度、功耗和速率初定每秒采样，读取成本较高的传感器可按实测降低频率。容量用字节整数，网卡链路速率用 b/s，网卡和磁盘吞吐率用 B/s。未发现的指标从清单省略，已发现但当次读不到时传 null。按样本时间区分缺样和过期，真实的零照常上报。现有 `min/max` 仍表示显示量程，走势图的历史统计单独计算。
- **USB 传输**。在现有 HID 功能上验证带版本、会话、序号和短指标 ID 的批量报告，设备清单独立分段，连接恢复后重发。按实测吞吐确定每批容量与采样周期。 [现有临时 HID](../../../software/app/ipkvm/README.md#副屏与遥测)

## 实施顺序

1. 用 Windows 只读探测器在截图机器和不同 CPU/GPU 组合上核对保留字段，覆盖核显、多网卡、缺传感器及热插拔。
2. 把精简后的清单、指标类型和缺样规则写入契约和 Mock，同步副屏布局，检查采集器只上报清单内指标。
3. 实现 Windows 采集器、HID 批量发送和 `kvmd` 接收。实板测多网卡独立流量、连接恢复、USB 吞吐和采样开销。
