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
| 已安装的 PawnIO 与官方签名 Ryzen 模块 | 已识别的 SMU 表提供 GHz 时钟域、核心温度、电压、功耗与限值。读取硬件 P-state 状态得到每个物理核心的倍频 |
| 已运行的 InpOutx64 驱动与可选的 InpOutx64.dll | 支持的 Ryzen 桌面平台上，从 FCH 内部 PLL 读取 BCLK 设置 |
| 已安装的 PawnIO 与官方签名 SMBus 模块 | AMD PIIX4 / Intel I801 可访问的 SPD5118 温度、匹配 DDR5 PMIC 的告警、总功率及六路电压 |

采集器通过 Windows 和本机驱动获取读数。硬件传感器没有可用读数时不会发占位值。DRAM 读写带宽和 Intel GPU 深度指标仍待实现。 [采集范围](../../../docs/superpowers/specs/2026-11-23-pc-telemetry-design.md)

## CPU 和 DIMM 硬件读数

- 将官方 `RyzenSMU.bin`、`AMDFamily17.bin` 和对应芯片组的 `SmbusPIIX4.bin` 或 `SmbusI801.bin` 放在 EXE 同目录。用有权访问 PawnIO 的用户运行。AM5 的 PM 表按处理器及固件表版本识别，当前支持 9950X3D、9800X3D，并为 5600X 提供部分全局指标。9950X3D 本机探测通过，另两种 Ryzen 及 Intel SMBus 路径需要对应机器复测。 [PawnIO 官方模块](https://github.com/namazso/PawnIO.Modules/releases/tag/0.2.12) · [Granite Ridge 与 Vermeer 表](https://github.com/Kyworn/gnr-smu/blob/master/gnr_smu/profiles.py) · [倍频与供电数据参考](https://github.com/irusanov/ZenStates-Core/tree/master/Hardware)
- 9950X3D 实测 FCLK 2000 MHz、UCLK/MCLK 3000 MHz，读到 16 个物理核心的倍频、PPT 限值 200 W、TDC/EDC 限值 160/225 A，以及 VDD、SoC 和 Misc 电流。DDR5 Hub `0x51`、`0x53` 实测 40.75 °C、38.50 °C，两条 PMIC 的告警位均清零。DIMM 按 SMBus 地址标识。 [SPD5118 手册与实现](https://github.com/torvalds/linux/blob/v6.18/drivers/hwmon/spd5118.c) · [PMIC 状态寄存器](https://datasheet.lcsc.com/lcsc/2501201522_Richtek-Tech-RTQ5132GQWF-00W-220_C3249964.pdf)
- **BCLK** 随超频指标上报。将 Highrez 发行的 `InpOutx64.dll` 放在 EXE 同目录。采集器先检查已有的 `inpoutx64` 设备能否打开，再调用 DLL 读取 FCH 寄存器。支持 Vermeer、Raphael、Granite Ridge 桌面平台的内部时钟模式。每五秒刷新 PLL 设置。外部时钟模式或驱动不可用时省略该字段。9950X3D 搭配 B850M AORUS ELITE WIFI6E ICE 实测 100 MHz，移除 DLL 后验证其余指标照常。其它主板与非默认频率待复测。 [寄存器解码参考](https://github.com/irusanov/ZenStates-Core/blob/869b7e82572337e5a673e92230edc92bddf0e44b/Hardware/Mmio.cs) · [Highrez DLL](https://www.highrez.co.uk/downloads/inpout32/)
- **DRAM 带宽**表示采样期间每秒实际读写的字节数。ZenStates-Core 的 UCLK、MCLK 和时序读取没有提供这个数据。当前 PawnIO AMD 模块也没有暴露 AMD Data Fabric 性能计数器的读写接口。 [ZenStates-Core 内存实现](https://github.com/irusanov/ZenStates-Core/tree/869b7e82572337e5a673e92230edc92bddf0e44b/Hardware/DRAM) · [PawnIO 寄存器列表](https://github.com/namazso/PawnIO.Modules/blob/0.2.12/AMDFamily17.p) · [AMD 性能计数器](https://docs.amd.com/r/en-US/57368-uProf-user-guide/4.2.-Performance-Monitoring-Counters-PMC)
- PMIC 已启用 ADC 时，按共享 SMBus 锁读取功率和对应采样通道。总功率只在 PMIC 处于总功率模式时报告。电压轮询暂时切换 ADC 通道，等待转换后恢复并核对原值。常驻采集每秒轮换一条电压通道，`--once` 一次读完六条。两条 DIMM 实测 VIN 5.04 V、SWC 1.80 V，并通过加锁的 ADC 原值恢复检查。电压通道只对已验证的 `0x850d`、`0x8c8a` 厂商标识启用。 [PMIC5100 寄存器表](https://studylib.net/doc/28499813/pmic5100-jesd301-2)

## GPU 核心热点和显存结温

- **显存结温**是显存自己的传感器。RTX 5090 走驱动 NVAPI 温度通道 2，原始值除以 256 得到摄氏度，本机普通权限实测 40 °C。Blackwell 的通道 1 是核心温度。 [NVAPI 实测与实现参考](https://github.com/GDuqueB/FanControl-Nvidia-Thermals/blob/main/docs/INVESTIGACION-NVAPI-RTX5090.md) · [分代通道映射](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/blob/master/LibreHardwareMonitorLib/Hardware/Gpu/NvidiaGpu.cs)
- **核心热点**是 GPU 芯片内部的最高有效测点。AMD ADL 和支持的旧 NVIDIA NVAPI 卡直接读取各自的热点。RTX 50 使用现有 PawnIO 读取有效的芯片温度通道并取最大值。将官方发布包里的签名模块 `Nvidia.bin` 放到 EXE 同目录，并以有权访问 PawnIO 的账户运行采集器。本机普通权限打开驱动返回 `0x80070005`，管理员权限复测在 RTX 5090 同时读到核心温度 29 °C、核心热点 31.1875 °C、显存结温 40 °C。 [签名模块发布包](https://github.com/namazso/PawnIO.Modules/releases/tag/0.2.12) · [温度模块源码](https://github.com/namazso/PawnIO.Modules/blob/0.2.12/Nvidia.p)

## USB 与 Panel

自动查找默认 VID/PID 的 KVMPanel 厂商 HID 集合，使用 64 字节 Output report 3。每个报告可包含多条 ASCII `指标ID=值`，以换行隔开。`!会话:采样毫秒` 心跳可在低采样频率下保持连接，`:设备ID=名称` 携带可读设备名。Windows 调整设置时开始新会话，板端清理上次的指标。需要本仓库新版 `kvmd` 才能接收批量和动态设备字段。物理 USB 的持续吞吐和热插拔尚待实板验证。 [板端接口](../ipkvm/README.md#副屏与遥测)
