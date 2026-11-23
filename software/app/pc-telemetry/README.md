# KVMPanel Windows 遥测采集器

原生 C/Win32 x64 应用。设置窗口可调整采样间隔（默认 1000 毫秒，范围 250–60000 毫秒）和采集内容。关闭窗口后在托盘继续采样。双击托盘恢复设置，右键可退出。应用设置保存在当前用户注册表。采样线程与 USB 发送独立，USB 带宽不足时发送最新的完整采样结果。

## 构建与运行

在 Visual Studio 的 **x64 Native Tools Command Prompt** 运行。

```bat
software\app\pc-telemetry\build.cmd
build\pc-telemetry\KVMPanelTelemetry.exe
```

`--tray` 从托盘启动。使用 PowerShell 执行只读探测，可查看本机实际获得的字段及两种 GPU 结温的可用性。

```powershell
Start-Process .\build\pc-telemetry\KVMPanelTelemetry.exe -ArgumentList '--once' -Wait -RedirectStandardOutput .\build\pc-telemetry\probe.json
Get-Content .\build\pc-telemetry\probe.json -Raw | ConvertFrom-Json
```

## 直接采集的读数

| 来源 | 当前实现 |
| --- | --- |
| Windows | CPU 总负载与逻辑处理器负载、Windows 报告的频率、物理内存已用和总量、每张网卡链路及收发速率、磁盘读写与支持时的 NVMe 温度 |
| NVIDIA 驱动 NVAPI / NVML | 每张卡的核心温度、显存结温、频率、占用、显存用量、整卡功耗与生效功耗限制，以驱动返回的字段为准 |
| AMD 驱动 ADL | 每张 Radeon 的温度、核心热点、显存温度、时钟、占用、电压或板级功耗，以 ADL 实际支持的字段为准 |
| 已安装的 PawnIO 与官方签名 NVIDIA 模块 | Blackwell 核心热点，要求有权限访问驱动 |

采集器直接调用 Windows 和显卡驱动的 DLL。HWiNFO、`nvidia-smi` 和其它监控进程不参与采样。CPU 供电读数与限值、BCLK/倍频、FCLK/UCLK、DRAM 带宽、DIMM 温度和 PMIC、Intel GPU 深度指标仍待实现。 [采集范围](../../../docs/superpowers/specs/2026-11-23-pc-telemetry-design.md)

## GPU 核心热点和显存结温

- **显存结温**是显存自己的传感器。RTX 5090 走驱动 NVAPI 温度通道 2，原始值除以 256 得到摄氏度，本机普通权限实测 40 °C。Blackwell 的通道 1 是核心温度。 [NVAPI 实测与实现参考](https://github.com/GDuqueB/FanControl-Nvidia-Thermals/blob/main/docs/INVESTIGACION-NVAPI-RTX5090.md) · [分代通道映射](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/blob/master/LibreHardwareMonitorLib/Hardware/Gpu/NvidiaGpu.cs)
- **核心热点**是 GPU 芯片内部的最高有效测点。AMD ADL 和支持的旧 NVIDIA NVAPI 卡直接读取各自的热点。RTX 50 使用现有 PawnIO 读取有效的芯片温度通道并取最大值。将官方发布包里的签名模块 `Nvidia.bin` 放到 EXE 同目录，并以有权访问 PawnIO 的账户运行采集器。本机普通权限打开驱动返回 `0x80070005`，管理员权限复测在 RTX 5090 同时读到核心温度 29 °C、核心热点 31.1875 °C、显存结温 40 °C。 [签名模块发布包](https://github.com/namazso/PawnIO.Modules/releases/tag/0.2.12) · [温度模块源码](https://github.com/namazso/PawnIO.Modules/blob/0.2.12/Nvidia.p)

## USB 与 Panel

自动查找默认 VID/PID 的 KVMPanel 厂商 HID 集合，使用 64 字节 Output report 3。每个报告可包含多条 ASCII `指标ID=值`，以换行隔开。`!会话:采样毫秒` 心跳可在低采样频率下保持连接，`:设备ID=名称` 携带可读设备名。Windows 调整设置时开始新会话，板端清理上次的指标。需要本仓库新版 `kvmd` 才能接收批量和动态设备字段。物理 USB 的持续吞吐和热插拔尚待实板验证。 [板端接口](../ipkvm/README.md#副屏与遥测)
