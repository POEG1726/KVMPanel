# 已下载源码

2026-11-20 下载，都是完整源码快照，只有浅历史。

| 名称 | 版本/参考分支 | 来源 |
|---|---|---|
| Linux | 6.18.64 | https://github.com/gregkh/linux |
| U-Boot | 2026.10 | https://github.com/u-boot/u-boot |
| Buildroot | 2025.02.20 LTS | https://github.com/buildroot/buildroot |
| Tina 框架 | master，当日快照 | https://github.com/100askTeam/Tina5-LinuxSDK |
| T113-i overlay | 当日快照 | https://github.com/DongshanPI/T113i_DevKitF_Tina5SDK |

D 盘本地镜像在 `repos/<名称>.git`；Ubuntu WSL 中对应的源码目录在 `~/work/KVMPanel/upstream/<名称>`，名称分别为 `linux`、`u-boot`、`buildroot`、`tina-framework`、`tina-t113i-overlay`。

`donors/` 保存参考代码和厂商资料。`donors/tina-t113i-devkit-default/` 中是从百问网 T113i DevKit 默认固件提取的 Boot0、FES1、U-Boot 和配置文件，提取来源见该目录的 `source-url.txt`。

Tina 框架和 overlay 不是完整 SDK，尚缺配套 kernel、brandy/DDR、platform、prebuilt 等包。后续需要时再补，不影响先研究 6.18。

辅助核参考保存在 WSL 的 `~/work/KVMPanel/upstream/`。`t113-amp-reference` 对应 MiniEVM 6.18 内核，`t113-amp-demos` 对应同一开发板的回环示例，`t113-c906` 对应 C906 启动与 FreeRTOS 示例。交叉工具链放在同级的 `riscv64-elf-cross` 和 `xtensa-hifi4-cross`。 [MiniEVM 6.18 移植](https://github.com/nickfox-taterli/t113-linux/tree/4e1c29c35) · [MiniEVM 回环示例](https://github.com/nickfox-taterli/t113-utils) · [C906 启动与 FreeRTOS 示例](https://github.com/nickfox-taterli/t113-c906)

上游源码不提交到主仓库；改驱动时把自己的补丁放 `software/os/`。
