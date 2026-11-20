# 计划 2：内核和驱动适配（交给 Codex 执行）

日期：2026-11-20。执行者：Codex CLI（WSL，`gpt-6.2-astra`，reasoning high，允许开 subagent）。指挥和审查：Claude。

## 先读这些

1. `AGENTS.md`、`README.md`
2. 设计文档 `docs/superpowers/specs/2026-11-20-kvmpanel-app-design.md`，重点是第 3 节（硬件）、7.1（驱动范围）、9（风险）
3. `docs/linux-6.18.md`（现状和差距）、`docs/development.md`（构建命令）
4. 硬件：`hardware/README.md`、网表 `hardware/exports/2026-11-20/Netlist_Schematic2_2026-11-20.tel`、数据手册 `hardware/reference/datasheets/`、面板参考包 `hardware/reference/display/TD4322.zip`

## 背景

- **板子还没到货，也还没上电过。**这个阶段只做能在电脑上验证的事：能编译，能通过静态检查，能在 QEMU 里跑的部分要跑通。所有需要实板的结论都标成“待实板验证”。
- **启动介质没定**，可能是 SPI NAND，也可能直接从 SD 卡启动、不贴 NAND。DTS 和 Buildroot 配置不要写死这一点。
- 上层应用（Rust 写的 `kvmd` 和副屏 `panel`）会用到这些接口：V4L2 采集节点、JPEG 编码（V4L2 M2M 或软件）、DRM 显示加背光、evdev 触摸和按键、configfs USB gadget、IIO 光照（BH1750）、GPIO（PC817 电源和复位）、hwmon 或 thermal（SoC 温度）、看门狗、SD 卡块设备。

## 本阶段目标（按优先级）

1. **USB gadget**：内核打开 configfs 和 `f_hid`、`f_mass_storage`、`f_ncm`、`f_rndis`、`f_acm`。写一个板上用的 gadget 组装脚本，放在 `software/os/`，按设计文档第 4.5 节组合各功能：
   - 键盘和鼠标分别做成 HID 接口。
   - 上位机数据作为厂商自定义的 HID 集合，挂在鼠标接口上，`no_out_endpoint=1`。
   - 存储做成多 LUN。
   
   在 QEMU 里用 `dummy_hcd` 验证默认组合能枚举，并能在 host 侧看到 HID 和存储。另外在文档里写明：T113-i 的 USB0 只有 5 个 IN 和 5 个 OUT 端点，`dummy_hcd` 测不出这个限制。
2. **视频采集**：
   - 把 LT8619C 驱动移植成 V4L2 子设备，参考 `~/work/KVMPanel/upstream/media-donors` 里 Rockchip 的 GPL 驱动，去掉私有依赖。
   - T113-i 并行 CSI 采集驱动：在 sun6i-csi 上扩展或新写，对照数据手册和 Tina VIN 的寄存器、DMA、IRQ。
   - 在本板 DTS 里加上节点和绑定。
   - 两个驱动都要能编译、通过 `checkpatch`。在文档里写清 8 位并行接口的带宽上限，以及 1080p60 和 1080p30 是否可行的计算。
3. **JPEG 编码**：
   - 调查 T113-i 在 6.18 上走硬件 JPEG 编码的路线，包括社区的 V4L2 编码补丁和 Cedar 编码 ABI，写出结论。
   - 不论结论如何，都在 Buildroot 里加上 libjpeg-turbo（带 NEON）作为保底方案。
4. **屏幕**：
   - TD4322 的 DRM 面板驱动，按参考包写初始化序列，4 lane、RGB888、1080×1920。
   - sun6i DSI 补齐 Command Mode 的帧更新和 TE 同步。
   - DTS 加上面板节点、复位 GPIO 和背光。
   - 面板型号和复位方式还没确认，不确定的地方写进文档。
5. **其他外设的 DTS 和内核配置**：
   - S3508 触摸（RMI4 I2C）。
   - BH1750（I2C，地址按网表）。
   - SD 卡（SMHC0，PF0–PF5）。在文档里提醒确认 VCC-PF 的电压。
   - THS 温度、看门狗。
   - PC817 电源和复位，用 GPIO，引脚按网表找出来。
   - SW1–SW4 还没接线，先在 DTS 里留注释占位。
6. **Buildroot**：在 `software/os/buildroot/configs/` 里加上上面需要的用户态包，至少包括 libjpeg-turbo、v4l-utils、evtest、libgpiod 工具、i2c-tools。保证 `kvmpanel_defconfig` 能完整构建，并且 QEMU 冒烟测试仍然通过。

## 约束

- 遵守 `AGENTS.md`：自己写的 DTS、配置、补丁和脚本放主仓库的 `software/os/`；上游源码放在 WSL 的 `~/work/KVMPanel/upstream/` 下，可以按需新建独立的工作树，比如沿用 `linux-dsi` 的做法；构建输出放 `~/work/KVMPanel/build/`。
- **主仓库里有用户还没提交的改动**（`git status` 里的 `M`、`D`、`??`）。一律不要动它们，也不要 revert。提交时只 `git add` 自己改的具体路径。
- 每完成上面的一项，验证通过后就在主仓库提交一次，提交信息用英文，结尾加一行 `Co-Authored-By: Codex <noreply@openai.com>`。**不要 push。**
- 不做任何硬件操作（没有板子）。不要用 sudo 改系统级设置；需要安装 apt 包时，在最终报告里列出来，不要自己装。
- 技术结论写进 `docs/linux-6.18.md`（按主题加小节），不要另开新文档。
- 可以开 subagent 并行处理互不相关的项，比如采集、屏幕、gadget。但同一个源码树、同一个文件，一次只能有一个 agent 在写。

## 完成标准

- 本地内核、DTB 和模块都能编译通过（命令写进 `docs/development.md`），新驱动没有编译警告，并且通过 `checkpatch --strict`（只需修掉 error 和 warning）。
- 修改后的 QEMU 冒烟测试通过，包括 gadget 和 `dummy_hcd` 的枚举。
- `docs/linux-6.18.md` 的结果表更新到最新，每一行写清现状和“待实板验证”的项目。

## 最终报告

最后一条消息用中文，内容包括：
- 每项的完成情况，以及对应的提交哈希。
- 执行过的验证命令和结果。
- 哪些需要实板验证。
- 遇到的阻塞，以及需要用户决定或安装的东西。
