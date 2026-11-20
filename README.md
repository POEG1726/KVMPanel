# KVMPanel

基于 Allwinner T113-i 的 **IP KVM + PC 机箱 Dashboard 副屏**项目。

- IP KVM：视频采集、远程画面、USB 键鼠和 PC 电源控制。
- Dashboard：本地屏幕显示 PC 状态、遥测与设备状态。
- 附加控制：BCLK/超频控制接口、Front Panel 状态、自身电源管理。

当前处于**开发基础设施与静态适配研究阶段**：尚无经实板验证的 Linux 6.18 镜像，也尚未实现应用。

## 从这里开始

1. 阅读 [HANDOFF.md](HANDOFF.md)，了解当前状态和下一步。
2. 阅读 [架构与边界](docs/architecture/system.md) 和 [平台决策](docs/decisions/0001-platform.md)。
3. 查看 [Linux 6.18 适配检查](docs/research/linux-6.18-support.md)。
4. 按 [环境准备](docs/bringup/environment.md) 获取源码和创建 Linux 工作目录。
5. 按 [Bring-up 顺序](docs/bringup/plan.md) 开发，不跳过基础验证。

仓库边界与提交规则见 [仓库管理](docs/repository.md)。

## 目录

| 目录 | 用途 |
|---|---|
| `hardware/` | EDA 工程归档、导出文件、参考资料和硬件发布记录 |
| `software/app/` | IP KVM、Dashboard 与公共设备服务的自有代码 |
| `software/os/` | Linux/U-Boot 配置、设备树、补丁和 Buildroot external tree |
| `software/firmware/` | STC 电源管理等辅助 MCU 固件与协议 |
| `docs/` | 架构、决策、适配研究和开发手册 |
| `handoff/` | 历史交接记录；当前入口始终是根目录 `HANDOFF.md` |
| `manifests/` | 上游源码版本锁定和环境约定 |
| `provenance/` | 下载、硬件资料与研究证据的来源记录 |
| `tools/` | 可重复执行的源码管理与工作目录准备工具 |
| `tests/acceptance/` | 硬件和系统验收项目及结果格式 |
| `sources/` | 本地上游源码镜像；大体积内容不进入本项目 Git |
| `build/`, `artifacts/`, `downloads/`, `logs/` | 本地构建、产物、缓存和日志，不进入 Git |

## 源码管理

自有代码、补丁、配置和文档使用本目录的 **一个 Git 仓库**。上游完整源码使用独立 Git 仓库，由 [sources.lock.json](manifests/sources.lock.json) 固定提交。

- Linux：`v6.18.64`，目标内核；完整源码快照，不下载全部历史。
- U-Boot：`v2026.10`，主线启动程序评估基线；实际 DDR 启动方案尚待验证。
- Buildroot：`2025.02.20` LTS，根文件系统构建工具基线。
- Tina：公开框架与 T113-i overlay 作为厂商参考。**完整匹配 SDK 尚未取得**。

```powershell
python tools/sources.py verify --full
python tools/sources.py fetch
```

完整工作树在 Ubuntu WSL 的 Linux 文件系统创建，避免 Windows 路径大小写与符号链接冲突。具体路径见 [环境文档](docs/bringup/environment.md)。

## 状态与发布

板上主芯片为 T113-i、K4B4G1646D、W25N04KW、RTL8211F、LT8619C，使用 STC8G1K08A 辅助管理电源。PCB 器件快照可见 `hardware/reference/board-components-20261120.json`，不代表实物已经装配或验证。

IP KVM 的视频采集和 JPEG/MJPEG 编码是当前主要适配缺口。T113-i 的 H.264 能力是解码，不能把它当作硬件 H.264 编码器。

本仓库目前仅初始化为本地仓库。上游源码保留原许可证；公开发布前需确认自有代码许可证，以及硬件、面板资料和厂商库的再分发权限。详见 [许可证管理](docs/licensing.md)。
