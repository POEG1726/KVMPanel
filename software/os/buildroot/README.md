# Buildroot external tree

使用 `BR2_EXTERNAL=<本目录绝对路径>`。当前使用 Buildroot 2025.02.20 LTS。

- `configs/kvmpanel_defconfig`：Cortex-A7 hard-float、Bootlin musl、静态 BusyBox，生成 `rootfs.cpio.gz`。
- `configs/kvmpanel-diagnostics.fragment`：叠加到同一 defconfig，在独立输出目录生成带 `memtester`、`ethtool`、`mtdinfo` 和 `lsmtd` 的上板诊断镜像。
- `board/kvmpanel/rootfs-overlay/`：自有 rootfs 文件，后续逐项加入。
- `package/`：自有应用与服务的 Buildroot 包。
- `package/kvmpanel-stc-power/`：STC8 串口状态和原厂 ISP 升级程序。配套 BusyBox 补丁通过 `BR2_GLOBAL_PATCH_DIR` 应用。 [构建与使用](../../firmware/stc8/README.md)
- `Config.in` / `external.mk` / `external.desc`：外部树入口。

内核单独构建，通过引导程序把 initramfs 传给内核。构建命令见 [开发说明](../../../docs/development.md)。

## QEMU 内存占用

2026-11-23 使用 2026-11-22 构建的 `buildroot-stc-power/images/rootfs.cpio.gz` 对应的完整用户态，包含 `kvmd` 和 STC8 工具。先测原来的 initramfs 方式，再把同一份用户态放入测试用 ext4 根分区。两种方式都挂载独立 ext4 `/data`，确认 `kvmd` 和 DHCP 已运行。用同一份 Debian 测试的已有 `/data` 再复测，避免把账号和默认配置首次初始化与常规启动混在一起。

与 Debian 使用相同的 Linux 6.18.64 测试内核、双核 Cortex-A7、512 MiB RAM 和 32 MiB CMA。每 200 ms 采样 `MemTotal - MemAvailable`。启动窗口至串口登录且 `kvmd` 启动约 3 秒后，随后再采 30 秒空闲。

| 根文件系统与数据状态 | 启动峰值 | 空闲平均 | 空闲峰值 |
|---|---:|---:|---:|
| Buildroot initramfs，初始 `/data` | 55.1 MiB | 46.0 MiB | 55.1 MiB |
| Buildroot initramfs，已有 `/data` | 71.7 MiB | 71.3 MiB | 71.7 MiB |
| Buildroot ext4，初始 `/data` | 22.9 MiB | 22.7 MiB | 22.9 MiB |
| Buildroot ext4，已有 `/data` | 50.0 MiB | 50.1 MiB | 50.1 MiB |
| Debian ext4，已有 `/data` | 108.6 MiB | 70.7 MiB | 76.5 MiB |

在已有 `/data` 的三次启动里，`kvmd` 在采样时点的峰值 RSS 均约 36.2 MiB。按同为 ext4 的结果，Buildroot 空闲平均比 Debian 少 20.6 MiB。原 Buildroot 以 initramfs 启动时，根文件系统占用 RAM，实测空闲平均接近 Debian 的 ext4 配置。各组均无活跃 KVM 会话，实板采集、编码和面板内存待测。 [Debian 原始测量](../debian/README.md#qemu-内存占用)
