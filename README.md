# KVMPanel

自己做着玩的 T113-i 小板，主要功能是 **IP KVM** 和 **PC 机箱 Dashboard 副屏**，顺带做 PC 开关机、复位和超频控制。

## 目录

- `hardware/`：硬件工程、导出和参考资料。
- `software/app/`：KVM 和副屏应用。
- `software/os/`：内核、U-Boot、Debian 和 Buildroot 的配置、设备树和补丁。
- `software/firmware/stc8/`：独立构建的 [STC8 电源管理固件](software/firmware/stc8/README.md)。
- `software/firmware/t113-amp/`：T113 AMP 固件。
- `docs/`：必要的开发说明和适配笔记。
- `sources/`：已下载的上游源码，本地保存，不提交进这个仓库。

SD 卡系统采用板级 Linux 6.18 和 Debian 13 armhf，预装 Vim。USB FEL 诊断可以加载 Buildroot initramfs。参见 [Debian 镜像](software/os/debian/README.md)、[开发构建](docs/development.md)及 [6.18 适配笔记](docs/linux-6.18.md)。
