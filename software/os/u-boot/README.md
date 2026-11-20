# 启动程序适配

主线 v2026.10 仅为评估源码。当前没有本板验证后的配置。

- `configs/`：验证后的板级 defconfig、DDR 参数来源和启动布局。
- `patches/`：针对 U-Boot v2026.10 的补丁。

优先使用可工作的厂商 DDR 初始化参考；核对外置 K4B4G1646D、SPI NAND 启动、加载地址和 PSCI。启动方式验证不能由 Linux 中存在 Flash 驱动替代。
