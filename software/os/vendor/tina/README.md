# Tina 参考资料边界

锁文件中的两个公开仓库分别是 SDK 框架与 T113-i overlay，不是完整 Tina SDK。它们不能单独编译厂商镜像。

待取得并核对：

- 同一版基础 SDK（含完整 kernel、brandy/DDR 初始化、device、tools）。
- 匹配的 buildroot/openwrt、platform、prebuilt/toolchain、必要固件与用户态库。
- 下载来源、原始文件名、版本、SHA-256 和使用/分发条件。

官方教程入口：
https://dshanpi.100ask.org/docs/T113i-Industrial/part3/DevelopmentEnvironmentSetup/

原始包放 `downloads/`，清单放 `provenance/`。不得把 framework/overlay 的成功下载当作完整 SDK 就绪。当前目标系统仍为 6.18，厂商 5.4 用作适配参考。
