# 计划 5：KVM 后端 kvmd（交给 Codex 执行）

日期：2026-11-20。执行者：Codex CLI（WSL，`gpt-6.2-astra`，reasoning high，完全权限，允许开 subagent）。指挥和审查：Claude。

## 先读这些

1. `AGENTS.md`、`README.md`
2. 设计文档 `docs/superpowers/specs/2026-11-20-kvmpanel-app-design.md`，重点是第 4.5、6、7.3、7.4、10 节
3. **接口契约（唯一权威）**：`software/app/api/openapi.yaml`、`software/app/api/README.md`、`software/app/api/schema/*.json`
4. 参考行为：`software/app/api/mock/`。它的测试（`mock/test/*.test.ts`）就是契约的可执行说明
5. 硬件接口：`docs/linux-6.18.md`、`software/os/buildroot/board/kvmpanel/rootfs-overlay/usr/sbin/kvmpanel-gadget`、`software/os/linux/dts/`

## 目标

在 `software/app/ipkvm/` 用 Rust 实现 `kvmd`，**行为与契约和 mock 一致**，可以直接替换 mock 给网页前端和副屏使用。

## 要求

- **语言和依赖**：Rust stable，具体 crate 自己选（axum + tokio 是推荐方案）。可以用 rustup 把 Rust 装到 WSL 用户目录，不要用 sudo，也不要动系统级设置。
- **两种运行模式**：
  - `--sim`：在 PC 上运行，不碰任何硬件。视频用测试图案生成 JPEG，遥测数据和 mock 一致，文件管理用一个本地目录模拟 sd、share、system 三个根。网页前端会用它做联调。
  - 默认模式：在板子上运行，通过下面这些接口访问硬件：
    - 视频：V4L2 采集，加 libjpeg-turbo 编码。
    - USB gadget：configfs，用 `/dev/hidgN` 收发键鼠和遥测，大容量存储的 LUN 绑定镜像文件。
    - 电源键和复位：libgpiod 控制 PC817。
    - 传感器：BH1750 通过 IIO 读取，SoC 温度从 thermal 读取。
    - 背光：sysfs。
  - 硬件部分用 trait 隔开，sim 和真机各有一套实现。真机实现现在只要求能编译，并且有单元测试，因为板子还没到。
- **端点和消息**：openapi.yaml 里的每个端点都要实现；`/api/ws`、`/api/stream`、`/api/terminal` 和 panel socket 的全部消息、关闭码、错误码都要实现。终端用真实的 PTY。
- **端点预算和 gadget**：USB 端点预算必须和 `usbBudget` 的规则一致。`/api/usb/apply` 调用或复用 `kvmpanel-gadget` 的逻辑重建 gadget。
- **遥测**：上位机通过 HID 厂商报告（report ID `0x03`，63 字节，走 EP0 SET_REPORT）发数据，**具体的数据格式还没定**。所以做一个可替换的解码层：现在先接受一个简单的占位格式（自己定，写进 `software/app/ipkvm/README.md`）；sim 模式直接用生成器产生数据。
- **持久化**：配置、账号、WoL 列表、脚本、MCP key 和 logo 存到 `/data/kvmd/`，sim 模式下存到本地目录。写入要原子化（先写临时文件再 rename）。密码用 bcrypt 或 argon2。
- **网页静态文件**：从一个可配置目录提供，以后会放网页前端的构建产物。
- **测试**：
  - 用 Rust 集成测试覆盖每一组端点，至少要有 mock 测试里同样的断言。
  - 计划 1 的 Review Focus 五项必须有测试：登录退避和锁定；非法配置原样拒绝；`..` 路径；上传超出空间或中途断开；坏的 WebSocket 消息。
  - 另外要有会话失效时关闭 socket 的测试，覆盖 4401、4403、4409 和 1012。
- **板上构建**：
  - 交叉编译到板子用的目标（Buildroot 的 ARMv7 hard-float musl 工具链，见 `docs/development.md`）。
  - 在 `software/os/buildroot/package/kvmd/` 加 Buildroot 包，并接入 `kvmpanel_defconfig`。
  - QEMU 冒烟测试里要启动 `kvmd --sim`，并在镜像里用 `curl /api/health` 验证。

## 约束

- **契约不能私自改。**如果发现契约有矛盾、遗漏或者实现不了的地方，写进 `software/app/api/ISSUES.md`（说明问题、建议改法、影响面），先按最合理的理解实现，并在最终报告里列出来，由 Claude 决定是否改契约。
- **不要改** `software/app/web/`、`software/app/dashboard/`、`software/app/api/`（`ISSUES.md` 除外）。这几个目录由 Claude 同时在开发。
- 遵守 `AGENTS.md`。每完成一个里程碑，验证通过后在 `main` 本地提交一次，提交信息用英文，结尾加 `Co-Authored-By: Codex <noreply@openai.com>`。只 `git add` 自己的路径。**不要 push。**
- 不做任何硬件操作。
- 可以开 subagent 并行处理互不相关的部分，但同一个文件同时只能有一个 agent 在写。

## 建议的里程碑顺序

1. crate 骨架、`--sim`、持久化、`/api/health`、登录账号权限、统一配置和 USB 预算
2. `/api/ws`、`/api/stream`（sim 测试图案）、电源、WoL、EDID
3. 存储、文件管理、虚拟介质、下载任务、共享盘
4. 终端（PTY）、脚本、设备、网络、Tailscale、升级、MCP、品牌
5. panel socket 和遥测解码层
6. 真机硬件实现（能编译，有单元测试）
7. 交叉编译、Buildroot 包、QEMU 冒烟测试

## 最终报告

最后一条消息用中文，内容包括：
- 每个里程碑的提交哈希。
- 执行过的测试命令和结果。
- 在 PC 上启动 `kvmd --sim` 的命令（包括端口），供网页前端联调。
- `ISSUES.md` 里的契约问题。
- 哪些需要实板验证。
- 遇到的阻塞。
