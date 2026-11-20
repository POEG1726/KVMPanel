# LVGL 原型笔记

一次性 spike（计划 `docs/superpowers/plans/2026-11-20-04-panel-prototypes.md` Task 1），画面按 `e2-v5.html` 主屏。

## 来源

- LVGL v9.6.0（commit 80ca777e），https://github.com/lvgl/lvgl ，克隆在 WSL `~/work/KVMPanel/upstream/lvgl`，已记入 `sources/README.md`。
- cJSON v1.7.19，https://github.com/DaveGamble/cJSON ，`cJSON.c`/`cJSON.h`/`LICENSE`（MIT）原样放在 `third_party/cjson/`。
- 字体：运行时从 `/mnt/c/Windows/Fonts` 读 Segoe UI（`segoeui.ttf`、`seguisb.ttf`），用 LVGL tiny_ttf 渲染；找不到就用 DejaVu。Segoe UI 不能随板子分发，正式版要换一套可分发的字体。

## 构建和运行

```sh
# Windows 侧启动 mock（panel socket 在 127.0.0.1:7701；WSL 是 mirrored 网络，直接连 127.0.0.1）
npm install --prefix software/app/api/mock
npm start --prefix software/app/api/mock

# WSL
cmake -S /mnt/d/MyProj/KVMPanel-panel/software/app/dashboard/prototypes/lvgl -B ~/work/KVMPanel/build/proto-lvgl -G Ninja
cmake --build ~/work/KVMPanel/build/proto-lvgl
~/work/KVMPanel/build/proto-lvgl/proto_lvgl
```

环境变量见 `main.c` 开头：`KVM_SCALE`（窗口缩小倍数，默认 2，即 540×960 窗口，渲染仍是 1080×1920）、`KVM_FPS`（默认 30，0 为不限）、`KVM_BLUR=0`（关掉面板背景模糊）、`KVM_SNAPSHOT`/`KVM_SNAPSHOT_AFTER`（按全分辨率写 PPM）、`KVM_EXIT_AFTER`。

截图：`KVM_SNAPSHOT=/tmp/lvgl.ppm KVM_SNAPSHOT_AFTER=65`（等曲线攒满 60 秒），再 `python3 ppm2png.py /tmp/lvgl.ppm ../screens/lvgl.png`。截图走 `lv_snapshot_take`，所以是 1080×1920 原尺寸。

## 实现要点

- 显示：没有用 LVGL 自带的 X11 驱动，因为它的窗口和显示分辨率一样大，1080×1920 放不进屏幕。`x11_out.c` 用 DIRECT 模式渲染整帧 1080×1920，再 2×2 平均后 `XPutImage` 到 540×960 的窗口。
- 渲染：单线程软件渲染（`LV_OS_NONE`、1 个 draw unit），XRGB8888。
- 背景：三个模糊色块启动时各渲染成一张 1392×1392 的 ARGB8888 图（圆盘加高斯模糊的径向解析式），每帧只改位置，用 additive 混合画在屏幕对象的 `DRAW_MAIN_END` 里。网格用细矩形逐帧画，上下 30% 渐隐。亮度 = .3 + .5 × max(cpu.load, gpu.load)。
- 面板：半透明底、细边框、圆角，开启 LVGL 9.6 的 `blur_backdrop`（半径 15）模拟 `backdrop-filter: blur(6px)`。
- 小曲线：每条一个 ARGB canvas，只在数据到达时（1 Hz）重画（网格线、面积填充、折线），平时每帧只贴图。范围固定为 metric 的 min..max。
- 超过 `warn` 的数值按 1.2 s 周期在正常色和 #ff6b6b 之间闪。
- socket：非阻塞 TCP，在 LVGL timer 里每 20 ms 轮询，按行用 cJSON 解析；断开后 0.5 s 起指数退避，最长 5 s。

## 帧率和 CPU（PC：9950X3D，WSLg，1080×1920 全分辨率渲染）

每次运行 100 秒，第 66 秒起用 `top -b -d 1 -n 30 -p PID` 采 30 次取平均（100% = 一个核）。FPS 是程序每 5 秒打印一次的完成帧数。

| 配置 | FPS | 进程 CPU |
|---|---|---|
| 30 FPS 上限，带面板模糊（默认） | 30.2 稳定 | 46.6% |
| 30 FPS 上限，无模糊 | 30.2 稳定 | 42.8% |
| 不限帧率，无模糊 | 62–83 | 98%（单核跑满） |
| 不限帧率，带模糊 | 61–67 | 98% |

整屏每帧都要重画，因为背景一直在动：一帧大约 13–16 ms 单核时间，大头是三张大色块图的 additive 混合。板子上（T113 双核 A7）会慢得多，还没实测；到时可能需要降帧率、只在背景局部重画，或者把背景改成低分辨率合成后放大。

## 和草图的差距

- 色块只平移，没有 CSS 里的 `scale()` 缩放动画（缩放画大图太慢）；混合用 additive 近似 `mix-blend-mode: screen`，重叠处略亮。
- 色块的模糊是每个圆单独预渲染，而草图是三个圆叠完再一起模糊，边缘过渡略有不同。
- 状态标签的图标是按 SVG 坐标用线和框手画的，比草图略粗。
- Core 柱和进度条的过渡用 LVGL anim（ease-out 600 ms），和 CSS `ease` 接近但不完全一样。
- 没做“等待电脑数据”画面（`pc` 消息未处理），协议也没有改。
- 字号和间距按 420→1080 的 2.57 倍换算；面板外边距和间距按要求收窄（16 px / 14 px）。
