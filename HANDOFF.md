# 接着做什么

更新：2026-11-20。

项目在 `D:\MyProj\KVMPanel`，主要做 IP KVM 和机箱 Dashboard 副屏。

## 现在的状态

- 本地 Git 仓库已建立，暂未配置远程。
- Linux 6.18.64、U-Boot 2026.10、Buildroot 2025.02.20 已下载完整源码快照。
- Tina 下载了公开框架和 T113-i overlay，完整 SDK 还没有。
- D 盘保存源码镜像，WSL Ubuntu 的 `~/work/KVMPanel/upstream/` 有完整工作目录。
- WSL 里有 Git 和 Python，但还缺 make、GCC、bison、flex 等编译依赖。
- 硬件手册和 PCB 器件清单已经复制过来。原生 EDA 工程导出被扩展权限阻止，尚未取得。
- 还没有本板 DTS、可用系统镜像、应用或 STC 固件，没做实板测试。

## 下一步

1. 拿到当前原理图、网表和屏幕资料，确认接线。
2. 准备编译依赖和启动程序，先让 6.18 通过串口进 shell。
3. 逐步跑通网络、USB、屏幕和触摸，再做 HDMI 采集与 JPEG 编码。

具体缺口看 `docs/linux-6.18.md`，源码版本和位置看 `sources/README.md`。
