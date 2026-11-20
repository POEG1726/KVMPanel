# 0001：平台与源码管理基线

日期：2026-11-20。状态：已采用，运行适配待验证。

## 决策

- 项目名 KVMPanel；目录 `D:\MyProj\KVMPanel`。
- 使用 Linux 6.18 LTS，初始锁定 v6.18.64，不启用 PREEMPT_RT。
- U-Boot v2026.10 作为主线评估源码；优先保留已验证的厂商 DDR 初始化/启动链，再迁移启动程序。
- Buildroot 2025.02.20 LTS 作为自有根文件系统构建基线，使用 `BR2_EXTERNAL`；不直接修改上游 package 作为唯一改动存储。
- Tina 框架与 T113-i overlay 是参考资料。完整 SDK 未取得，不能称为可构建的厂商基线。
- 上游下载完整固定提交快照，浅历史；需要分析历史时按需 deepen，不下载所有分支和全部历史。
- D 盘保存 bare Git 源码镜像，Ubuntu WSL 的 ext4 保存可编辑/可构建工作目录。

## 理由与限制

控制功能没有已确认的硬实时截止时间。LTS 能降低维护频繁换大版本的成本，但并不自动补齐 T113-i 的板级配置、CSI 或编码驱动。

完整主线源码的工作树可恢复，支持补丁维护；厂商代码按需作为寄存器、时序和旧 ABI 的参考。不能假定旧 `.ko` 或用户态编码库可直接用于 6.18。

## 参考

- https://www.kernel.org/category/releases.html
- https://buildroot.org/download.html
- https://github.com/u-boot/u-boot/tree/v2026.10
- https://dshanpi.100ask.org/docs/T113i-Industrial/part3/DevelopmentEnvironmentSetup/
