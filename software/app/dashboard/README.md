# Dashboard

板端使用 LVGL 9 和 Linux framebuffer。CPU、GPU、内存和系统页显示 `kvmd` 实际收到的指标。`Display` 页可旋转屏幕或逐项选择指标，配置与 KVM 网页共享。电脑遥测离线时显示板子自身状态，读取到关机的电源灯状态时可短按开机。离线息屏后，第一下触摸只唤醒屏幕。

## 构建与启动

用 LVGL v9.6.0 源码和现有 Buildroot 的 ARM 工具链构建静态二进制。cJSON v1.7.19 沿用 LVGL 原型附带的源码和许可证。

```sh
work="$HOME/work/KVMPanel"
host="$work/build/buildroot-plan2/host/bin"
cmake -S software/app/dashboard/lvgl -B "$work/build/dashboard-board" -G Ninja \
  -DCMAKE_SYSTEM_NAME=Linux -DCMAKE_BUILD_TYPE=MinSizeRel \
  -DCMAKE_C_COMPILER="$host/arm-linux-gcc" -DCMAKE_CXX_COMPILER="$host/arm-linux-g++" \
  -DCMAKE_EXE_LINKER_FLAGS=-static
cmake --build "$work/build/dashboard-board"
```

板端默认打开 `/dev/fb0` 与 `/run/kvmd/panel.sock`。`KVMPANEL_FRAMEBUFFER` 和 `KVMPANEL_PANEL_SOCKET` 可覆盖路径，`KVMPANEL_TOUCH` 指向实际触摸输入设备。LVGL 使用单个 24 扫描行绘制缓冲，帧缓冲由内核提供。可用 `KVMPANEL_HEADLESS=1` 在无需 framebuffer 的环境运行模拟。 [LVGL 源码](https://github.com/lvgl/lvgl)

Debian 镜像可带上 Dashboard 服务，详情见 [镜像构建](../../os/debian/README.md)。原生及 ARM/QEMU 已验证 Unix socket 接收、动态指标、离线状态和四个屏幕方向。实板触摸、色彩、渲染速率及内存峰值待测。

原型在 `feat/panel-proto`、`feat/panel-slint` 和 `feat/panel-qt`。板端采用按组分页的指标列表。原型动画、曲线排版和横屏方案 B 留在对应分支。 [软件进度](../../../docs/software-progress.md)
