# 计划 5b：kvmd 审查修复和契约裁定（交给 Codex）

日期：2026-11-20。约束和计划 5 相同：只改自己的路径，每项验证后在 `main` 本地提交，不 push。

## A. 审查的 Important 问题（必须修，每项都要有回归测试）

1. **广播落后时，连接会被误判为 4409。**
   - 问题所在：`console.rs:573` 和 `console/video.rs:114` 把 `RecvError::Lagged` 当成 4409 关闭连接；`console.rs:591` 每个事件都重算一次完整 status，每次要 fork 两次 `gpioget`，还要等视频锁。
   - 后果：用户按住一个键时，系统自动重复发送按键，可能直接把用户踢下线。
   - 修法：
     - 遇到 Lagged 时，重新做一次权限判断，然后推一份最新 status，不要关闭连接。
     - `status_changed` 改成“脏标记”，最多每秒刷新 10 次。
     - 缓存电源灯和硬盘灯的读数。
   - 测试：模拟每秒 100 次按键事件，连续 5 秒，连接不能断。
2. **在异步运行时里做阻塞操作，同时还拿着 std 的锁。**
   - 问题所在：`config.rs:178-181`、`panel.rs:226` 在拿着 cfg、storage 和 hid 锁的情况下，同步执行 `kvmpanel-gadget`；另外 `panel.rs:204` 的 fsync、`storage.rs:539-556` 的删除、`storage.rs:402-412` 的上传写入，也都是阻塞操作。
   - 修法：
     - gadget 重建和文件操作放进 `spawn_blocking`。
     - 给 apply 单独一把锁，执行脚本期间不拿 cfg 锁。
     - 配置改成放在 `RwLock` 后面的 `Arc<Value>`。
   - 测试：apply 期间（sim 模式下用一个睡 2 秒的假 gadget 命令），`/api/health` 和 panel socket 的 `get_config` 都能在 200 ms 内响应。
3. **升级后，存储的旧状态可能让 kvmd 启动不了。**
   - 问题所在：`config.rs:10-19` 遇到不符合当前 schema 的 `config.json` 或 `usb-applied.json` 就拒绝启动；`Settings`（`system.rs:30-39`）没有 `#[serde(default)]`；`update.rs:252-316` 切换版本时没有回滚。
   - 修法：
     - 把存储的值合并到默认值上；不合法的分区丢掉，回到默认，并打一行日志。
     - 补上 serde 的默认值。
     - 升级时保留上一个版本；新版本在 60 秒内没有通过健康检查，就自动回滚。
   - 测试：用缺字段、多字段、类型错误的旧 `config.json` 启动，都能正常起来并返回默认值。

## B. 契约裁定要改的地方（见 `software/app/api/ISSUES.md` 最后一节）

4. **EDID reset**：写入板级默认 EDID，也就是驱动里那份首选 1080p30 的，不再写 1080p60。
5. **paste**：
   - 上限是 UTF-8 编码后 64 KiB。
   - 当前键盘布局打不出的字符，整条拒绝，回 `{"t":"error","error":"unmappable"}`（`ws.schema.json` 已经加了这个错误码）。
6. **共享盘**：导出期间，**电脑一侧可读写**（LUN2 不再只读），板子一侧只读。有正在写入的上传或下载时切换，回 409 `share_busy`。
7. **设备故障**：硬件缺失、gadget 重建失败时，返回 `503 {error:"device_unavailable"}`。
8. **Tailscale**：`GET /api/tailscale` 增加可选字段 `error`，记录上一次异步动作的失败原因，下一次动作开始时清空。
9. **介质**：storage 被关闭时挂载介质，返回 409 `in_use`，不要返回 500。

## C. 顺手修的小问题

- **登录**：
  - 失败记录的表要有上限，比如最多 1024 个用户名，满了就淘汰最旧的。
  - 不存在的用户也做一次假的 bcrypt，避免从响应时间看出用户名是否存在。
  - 同一时间最多 2 个 bcrypt 校验在跑。
- **HID**：`/dev/hidgN` 的文件描述符保持打开，收到 `usb_reenumerated` 时再刷新；鼠标每次移动都不要再读 configfs。
- **gadget 脚本**：在“相对鼠标 + 遥测、没有绝对鼠标”的组合下，相对鼠标要保留 boot 协议，让 BIOS 还能用；这时遥测按计划 2 的修法，改用独立的 HID。
- **清理**：
  - 已结束的下载任务保留最近 20 个，其余删掉。
  - `apps/` 下只保留当前版本和上一个版本。
  - 下载设置 60 秒的读超时。
- **视频**：发送超时断开时，用关闭码 1011，不要直接断开。

## 完成标准

- `cargo test --locked --all-targets` 全部通过，新增的回归测试都有。
- ARM 交叉编译、QEMU 冒烟测试和 gadget 冒烟测试都重新跑一遍。

## 最终报告

用中文，写清每一项对应的提交和验证结果。
