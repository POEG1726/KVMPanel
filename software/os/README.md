# 系统适配层

- `linux/`：自有 DTS、内核配置、针对锁定基线的补丁。
- `u-boot/`：启动程序配置与补丁；DDR/启动介质验证归此层。
- `buildroot/`：BR2_EXTERNAL 根目录，维护自有 rootfs 配置、overlay 与 package。
- `vendor/tina/`：厂商 SDK 版本/布局说明；大源码放独立源仓库。

当前没有板级 defconfig 或可启动镜像。不要把其他 T113-S 开发板的 defconfig 当成本板已适配配置。
