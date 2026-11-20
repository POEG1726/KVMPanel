# 副屏三框架对比原型 Implementation Plan

> **For agentic workers:** 这是一个 spike（对比原型），按 brainstorming 的规则属于“throwaway prototype”，不走 TDD。目标是做出能看、能测帧率的东西，不追求代码质量。选定的框架之后另写正式计划。

**Goal:** 用 LVGL 9、Slint、Qt 6 Quick 各做一版副屏主屏（第 4.1 节 / 草图 `e2-v5.html`），在 PC 上以 1080×1920 运行，接 mock 的 panel socket 拿实时遥测数据。给用户比较观感，同时记录 CPU 占用和帧率，为板上实测做准备。

**Architecture:** 三个原型各自独立，放在 `software/app/dashboard/prototypes/{lvgl,slint,qt}/`。每个原型都：通过 TCP 连接 `127.0.0.1:7701`，按行读 NDJSON；按指标 id 填充主屏布局。主屏以竖屏 1080×1920 显示（窗口可以缩放到屏幕能放下的大小）。

**Tech Stack:** LVGL 9.x（C，X11 后端，WSLg 显示）、Slint 1.x（Rust，软件渲染器）、Qt 6 Quick（QML，强制 `QT_QUICK_BACKEND=software` 模拟没有 GPU 的情况）。

**Spec:** `docs/superpowers/specs/2026-11-20-kvmpanel-app-design.md` 第 4.1、7 节；协议见 `software/app/api/README.md` 的 panel socket 一节。

## Global Constraints

- 画面以草图 `e2-v5.html` 为准，内容包括：
  - E2 背景：缓慢漂移的模糊色光，负载越高，色光越亮；不画背景网格（用户后来去掉了）。
  - 半透明面板。
  - CPU 面板：16 根核心负载柱，标题行右侧有状态栏。
  - 每一行：名称 | 带边框的小曲线 | 数值。
  - 内存面板：A1/A2/B1/B2 合在一行，下面一张大图表。
  - 没有时钟和日期，也不显示风扇转速。
- 文字用英文（副屏默认英文）。
- 所有渲染都走 CPU：Qt 用 software backend，Slint 用 software renderer，LVGL 本来就是软件渲染。
- 原型不改 `software/app/api/`；如果需要改协议，先记下来。

## 需要用户先装的包（WSL，需要 sudo）

```sh
sudo apt-get install -y libsdl2-dev libfontconfig1-dev qt6-base-dev qt6-declarative-dev qml6-module-qtquick qml6-module-qtquick-window qml6-module-qtquick-shapes qml6-module-qtqml-workerscript
```

Rust 工具链（rustup）由 Codex 装到用户目录，Slint 原型共用它。

---

### Task 1: 共用部分和 LVGL 原型

- LVGL 源码固定为 v9.x 的一个 tag，克隆到 `~/work/KVMPanel/upstream/lvgl`（不进主仓库）。按 `sources/README.md` 的格式记下版本和来源。
- `prototypes/lvgl/`：`CMakeLists.txt`、`main.c`、`lv_conf.h`，以及 panel socket 客户端（POSIX socket、NDJSON 行解析、cJSON 或手写的最小解析）。
- 背景实现：预先渲染好模糊色光的图片，每帧只移动位置。
- 验证：`cmake --build` 编译通过；启动 mock 后运行原型，WSLg 弹出窗口，数据每秒刷新。截图存到 `prototypes/screens/lvgl.png`。

### Task 2: Slint 原型

- `prototypes/slint/`：Cargo 项目，`ui/dashboard.slint` 加 `src/main.rs`（tokio 或 std 线程读取 socket，用 `invoke_from_event_loop` 更新模型）。
- 渲染：`SLINT_BACKEND=winit-software`。
- 验证：`cargo run --release` 能跑，截图存到 `prototypes/screens/slint.png`。

### Task 3: Qt Quick 原型

- `prototypes/qt/`：`CMakeLists.txt`、`main.cpp`（用 `QTcpSocket` 读 NDJSON，再暴露给 QML 一个模型）、`Dashboard.qml`。
- 背景：用 `Canvas` 或预渲染图片做模糊，因为 software backend 不支持 `MultiEffect`。
- 验证：设置 `QT_QUICK_BACKEND=software` 后运行，截图存到 `prototypes/screens/qt.png`。

### Task 4: 对比和交给用户

- 三个原型分别连续运行 60 秒，记录进程的 CPU 占用（`pidstat` 或 `top -b`）和帧率（各框架自带的 FPS 统计）。这是在 PC 上测的，只能当相对参考。
- 把三张截图并排放进一个对比页面，在浏览器里给用户看。用户按观感选一个；性能结论要等板子点亮后再实测。
- 把结论写进 `software/app/dashboard/README.md`：选了哪个、为什么、PC 上的数据，以及板上要测哪些项。
- 提交：`feat(dashboard): three-framework panel prototypes`
