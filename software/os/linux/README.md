# Linux 适配

锁定基线见 `manifests/sources.lock.json`。完整源码在本地 mirror 和 WSL 工作目录。

- `dts/`：板级 DTS/DTSI；当前待创建，需对照最新网表。
- `configs/`：内核配置/片段；当前没有本板验证后的 defconfig。
- `patches/`：按应用顺序编号的补丁，记录 upstream 基线、来源和验证状态。

首个目标是串口进入 shell、内存容量正确、双核上线；随后才接入视频链路。
