# Buildroot external tree

使用 `BR2_EXTERNAL=<本目录绝对路径>`。当前使用 Buildroot 2025.02.20 LTS。

- `configs/kvmpanel_defconfig`：Cortex-A7 hard-float、Bootlin musl、静态 BusyBox，生成 `rootfs.cpio.gz`。
- `configs/kvmpanel-diagnostics.fragment`：叠加到同一 defconfig，在独立输出目录生成带 `memtester`、`ethtool`、`mtdinfo` 和 `lsmtd` 的上板诊断镜像。
- `board/kvmpanel/rootfs-overlay/`：自有 rootfs 文件，后续逐项加入。
- `package/`：自有应用与服务的 Buildroot 包。
- `package/kvmpanel-stc-power/`：STC8 串口状态和原厂 ISP 升级程序。配套 BusyBox 补丁通过 `BR2_GLOBAL_PATCH_DIR` 应用。 [构建与使用](../../firmware/stc8/README.md)
- `Config.in` / `external.mk` / `external.desc`：外部树入口。

内核单独构建，通过引导程序把 initramfs 传给内核。构建命令见 [开发说明](../../../docs/development.md)。
