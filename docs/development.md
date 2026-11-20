# 开发说明

## 文件放哪里

项目：`D:\MyProj\KVMPanel`。

上游源码镜像：`sources/repos/`；可编辑源码：Ubuntu WSL 的 `~/work/KVMPanel/upstream/`。使用 WSL 的 Linux 文件系统编译，避开 Windows 大小写和符号链接差异。

自己的设备树、配置和补丁放 `software/os/`，应用放 `software/app/`，STC 固件放 `software/firmware/stc8/`。构建输出放 `~/work/KVMPanel/build/`，不混进源码。

## 先把环境补齐

现有 Ubuntu WSL 能运行，但还没安装 make、GCC/G++、bison、flex、dtc、pkg-config、cpio 等编译依赖。当前没有编译系统。

后续按需要安装编译依赖并选择 ARM 交叉工具链。不要把宿主 x86 GCC 当成 T113 编译器。

Buildroot 的外部目录是 `software/os/buildroot/`，已有最小骨架，尚无本板 defconfig。

## 做的顺序

先启动和内存，再网络/存储/USB，再屏幕/触摸，最后视频采集、编码和两个功能一起运行。

每次先确认眼前功能真的工作：例如两核是否上线、图片是否正确、HDMI 重插能否恢复、Controller 关机是否先保存数据再断电。发现问题时保留有用日志即可，不要求每个步骤做一套记录。
