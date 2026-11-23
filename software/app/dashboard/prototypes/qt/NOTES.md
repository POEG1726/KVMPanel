# Qt 6 Quick 原型

Qt 6.4.2（Ubuntu 系统包），强制 `QT_QUICK_BACKEND=software`（`main.cpp` 里也调用了 `setGraphicsApi(Software)`）。画面按 `e2-v5.html` 的主屏。截图：`../screens/qt.png`（1080×1920，`QQuickWindow::grabWindow()`）。

## 文件

- `main.cpp`：命令行参数、注册类型、帧计数（每 5 秒打印 `fps`）、截图。`--landscape` 把画布换成 1920×1080。
- `panelclient.*`：`QTcpSocket` 读 NDJSON，保存每个指标的最新值和 60 秒历史；处理 `telemetry`、`status`、`pc`、`board`、`config`（只取 `offline_sleep_min`）；断线后 0.5→5 s 退避重连，保留当前画面。`board` 里的收发速率、SoC 温度、CPU、内存也存成指标（`board.rx` 等），离线页的小曲线直接复用 `Sparkline`。3 秒没有 `telemetry` 也算上位机断开，不依赖 `pc` 消息。`powerOnPc()` 发 `pc_power_on`，在 `board.pc_power` 变成 `on` 之前按钮显示 Powering on…，被拒时显示 Refused。
- `sparkline.*`：`QQuickPaintedItem`，画带边框的小曲线和内存大图（固定范围 min..max，null 断线）。最新的点在最左边，旧点往右移出。
- `bloblayer.*`：E2 色光。位置按画布宽高的比例算，竖屏和横屏都能用；`calm` 把绿色那团换成青色，供离线页用。启动时用 3 次 box blur 预渲染三个模糊圆（1/4 分辨率），每个 tick 只移动、缩放，用 screen 模式混合进一张 270×480 的不透明小图，再拉伸到全屏。
- `Dashboard.qml`：窗口、主屏和离线页的切换、离线息屏倒计时（`offline_sleep_min`，到时黑屏，点击或按键唤醒）。`MainScreen.qml`：主屏，横屏时暂时把竖屏主屏缩小居中显示（横屏方案 B 还没做）。`OfflinePage.qml`：电脑离线页，竖屏一列、横屏两行三列，组件 `KvRow.qml`、`StatusBar.qml`。其余：`Backdrop.qml`（色光；网格已按设计隐藏）、`Tile.qml`、`MetricRow.qml`、`Bar.qml`、`Chip.qml`、`T.qml`。

## 构建和运行

```sh
cmake -S /mnt/d/MyProj/KVMPanel/software/app/dashboard/prototypes/qt -B ~/work/KVMPanel/build/proto-qt -G Ninja -DCMAKE_BUILD_TYPE=Release
ninja -C ~/work/KVMPanel/build/proto-qt
# 窗口：0.5 倍缩放（540×960）；WSLg 下用 xcb，Wayland 后端呈现很慢
QT_QPA_PLATFORM=xcb ~/work/KVMPanel/build/proto-qt/proto-qt --scale 0.5
# 截图（离屏，1080×1920，等 63 s 让曲线填满）
QT_QPA_PLATFORM=offscreen ~/work/KVMPanel/build/proto-qt/proto-qt --screenshot qt.png --delay 63000
```

参数：`--host`/`--port`（默认 `KVMPANEL_PANEL_HOST`/`KVMPANEL_PANEL_PORT` 或 127.0.0.1:7701）、`--scale`、`--fps`（背景 tick，默认 30）、`--duration`、`--landscape`。

### 电脑离线页

Mock 用 `KVMPANEL_MOCK_PC` 指定电脑初始状态（见 `software/app/api/README.md`）。在 `off` 状态点 Power on PC，页面依次变成 Waiting for host app，8 秒后上位机连上，切回主屏。

```sh
# Windows 侧：$env:KVMPANEL_MOCK_PC = 'off'; npm start --prefix software/app/api/mock
QT_QPA_PLATFORM=xcb ~/work/KVMPanel/build/proto-qt/proto-qt --scale 0.5 --landscape
QT_QPA_PLATFORM=offscreen ~/work/KVMPanel/build/proto-qt/proto-qt --landscape --screenshot qt-offline-l.png --delay 8000
```

截图：`../screens/qt-offline-p.png`、`../screens/qt-offline-l.png`（PC 关机）和 `../screens/qt-waiting-l.png`（等待上位机）。

## 性能（PC：9950X3D，WSLg，`top -b -d 1 -n 30`，单核百分比）

| 场景 | 背景 tick | 帧数/秒 | 进程 CPU |
|---|---|---|---|
| xcb 窗口 540×960 | 30 Hz | ~50 | ~39% |
| xcb 窗口 1080×1920 | 30 Hz | ~50 | ~57% |
| offscreen 1080×1920 | 30 Hz | ~50 | ~50% |
| offscreen 1080×1920 | 60 Hz | ~61 | ~106% |
| offscreen 1080×1920 | 15 Hz | ~40–60 | ~37% |
| offscreen 1080×1920 | 10 Hz | ~43 | ~20% |
| WSLg Wayland 窗口 1080×1920 | 30 Hz | ~20 | ~45% |

- 帧数是 `frameSwapped` 计数，包含只有局部变化的帧（核心柱和进度条的 0.6 s 过渡、Tctl 闪红）。整屏重绘的次数等于背景 tick。
- 背景每动一次就要整屏重绘。在 offscreen 1080×1920、30 Hz 下拆开测：底色和网格约 17%，色光放大约 9%，面板（半透明圆角框、约 70 个 Text、21 个曲线 pixmap）约 33%。
- 不放色光时只有约 10%，因为 software renderer 只重绘脏区。
- 第一版用三个大的预模糊 `Image` 直接移动和缩放，整屏 30 Hz 时 90–95%，主要花在大图缩放上，所以改成了小图合成后再放大。
- 试过给面板开 `layer.enabled`（software 后端支持），反而升到 95%：任何子项变化都会重渲整层。
- 到板子上，背景 tick 可能要降到 10–15 Hz，或者只在数据刷新时动背景。

## 和草图的差距

- 面板没有 `backdrop-filter: blur(6px)`（software 后端没有模糊）。背景本来就是糊的，所以看不太出来。
- 字体用 Ubuntu Sans 代替 Segoe UI，字更宽，CPU 标题和状态栏挨得很近。Qt 6.4 不支持 `font.features`，数字不是等宽（tabular-nums），数值跳动时会轻微晃动。
- 状态点的光晕是一个半透明大圆，不是真正的 box-shadow。
- 色光在 1/4 分辨率上合成后放大，过渡比 CSS 稍软。线宽、圆角按 1080/420 换算后取整。
- 启动时没有 60 秒历史，前 60 秒只画已有的点（草图是预先填满的）。
- 状态文字：主屏上 `partial` 只显示 “PC partial”，协议里没有说明是缺 HID 还是缺上位机。离线页上已知上位机没连，所以显示 “PC HID only”。
- 离线页的 PC 图标、电源按钮图标用 `Shape` 按草图的 SVG 路径画；转圈是 90° 弧线绕图标旋转。
- 按键焦点：电源按钮显示时默认拿到焦点（蓝色描边），回车或空格等于点击。FN 进设置还没做。
