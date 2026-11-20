# Buildroot external tree

使用 `BR2_EXTERNAL=<本目录绝对路径>`。组件基线为锁文件中指定的 Buildroot 2025.02 LTS 补丁版本。

- `configs/`：本板验证后保存的 defconfig，目前没有。
- `board/kvmpanel/rootfs-overlay/`：自有 rootfs 文件，后续逐项加入。
- `package/`：自有应用与服务的 Buildroot 包。
- `Config.in` / `external.mk` / `external.desc`：外部树最小骨架，尚不代表板级配置已完成。
