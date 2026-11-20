# KVMPanel

自己做着玩的 T113-i 小板，主要功能是 **IP KVM** 和 **PC 机箱 Dashboard 副屏**，顺带做 PC 开关机、复位和超频控制。

## 目录

- `hardware/`：硬件工程、导出和参考资料。
- `software/app/`：KVM 和副屏应用。
- `software/os/`：内核、U-Boot、Buildroot 的配置、设备树和补丁。
- `software/firmware/`：STC 辅助电源管理固件。
- `docs/`：必要的开发说明和适配笔记。
- `sources/`：已下载的上游源码，本地保存，不提交进这个仓库。

基础系统采用 Linux 6.18 和 Buildroot。参见 [构建方法](docs/development.md)、[T113-i 启动链](software/os/vendor/tina/README.md)和 [6.18 适配笔记](docs/linux-6.18.md)。
