# Slint 原型

主屏草图 `e2-v5.html` 的 Slint 1.19.0 实现（Rust，软件渲染器）。这是一次性的对比原型。

## 文件

- `ui/dashboard.slint`：布局、E2 背景（色光漂移和网格）、面板、小曲线、状态标签。
- `src/feed.rs`：panel socket 客户端。在线程里按行读取 NDJSON，保存每项 60 秒历史，生成曲线的 SVG path 字符串；断线后每秒重连一次。
- `src/blobs.rs`：模糊色光按 1/4 分辨率预先渲染（圆盘卷积高斯，σ≈103 px），由渲染器放大。只有亮度变化时才重建像素。
- `src/main.rs`：通过 `invoke_from_event_loop` 原地更新模型（长度不变时用 `set_row_data`，保证 `animate` 生效）；用 `Window::take_snapshot` 截图。

## 构建和运行（WSL）

```sh
cd software/app/dashboard/prototypes/slint
export CARGO_TARGET_DIR=~/work/KVMPanel/build/proto-slint
cargo build --release
# 窗口缩成 540×960 显示
SLINT_BACKEND=winit-software SLINT_SCALE_FACTOR=0.5 $CARGO_TARGET_DIR/release/dashboard-slint-proto
# 1080×1920 截图：等待 68 s，让曲线攒满 60 s 历史，截图后退出
SLINT_BACKEND=winit-software SLINT_SCALE_FACTOR=1 PROTO_SCREENSHOT=$PWD/../screens/slint.png PROTO_SCREENSHOT_AFTER=68 \
  $CARGO_TARGET_DIR/release/dashboard-slint-proto
```

- 默认连接 `127.0.0.1:7701`，可以用 `KVMPANEL_PANEL_ADDR` 改地址。WSL 开了镜像网络，可以直接连 Windows 上的 mock。
- 加上 `SLINT_DEBUG_PERFORMANCE=refresh_lazy,console`，每秒会打印一次帧率。

## 实测（9950X3D，WSLg，winit-software）

| 窗口 | FPS | 进程 CPU（`top -b -d 1 -n 30`，单核为 100%） |
|---|---|---|
| 1080×1920（scale 1） | 约 48 | 约 97% |
| 540×960（scale 0.5） | 约 62 | 约 40% |

- 色光漂移和网格滚动由 `animation-tick()` 驱动，所以每一帧都要重画整个画面。
- 在全分辨率下，单线程软件渲染器已经跑满一个核，每帧约 20 ms。
- 板子 CPU 比这台慢很多，上板时需要降低背景帧率，比如限制在 15 到 20 fps；或者把背景改成很少变化的静态层，只重绘数据区域。
- 软件渲染器支持局部重绘，但背景每帧都在动，局部重绘起不了作用。

## 与草图的差异

- 面板没有 `backdrop-filter: blur`，Slint 没有背景模糊。面板只是半透明叠在背景上，透出的色光边缘比草图清楚。
- 色光之间不能用 `mix-blend-mode: screen`，只能普通 alpha 叠加，重叠处颜色略暗。
- 整层的模糊由每块色光单独预先模糊来近似；色光缩放时，模糊半径也会跟着缩放。
- CSS `ease-in-out` 用余弦曲线近似。
- 网格的上下渐隐用每条横线各自的透明度、每条竖线的线性渐变来实现，没有 CSS mask。
- 字体用 Ubuntu Sans（WSL 里没有 Segoe UI）。没有 `tabular-nums`，好在这款字体的数字本身等宽。
- USB 设备名太长时会截断成省略号（mock 给的是 “USB Flash Disk”）。草图里是 “U 盘”，比较短。
- 曲线从左往右填，历史不满 60 s 时只占左边一部分。草图开局就预填了 60 个点。
- 未实现“等待电脑数据”的完整画面，只在底部显示一条提示。
