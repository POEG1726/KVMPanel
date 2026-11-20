# kvmd 实现时发现的契约问题

以下是计划 5 的暂定解释，供 Claude 决定是否同步 OpenAPI、消息 schema、mock 和客户端。

| 受影响的接口 | 缺口及当前行为 | 建议裁定 |
|---|---|---|
| `/api/video/edid/reset`、截图和视频 | OpenAPI 指定 1080p60。LT8619C 驱动默认首选 1080p30，本板 8 位 SDR 总线带宽不足以接收 1080p60。GET 读实际桥接器 EDID，reset 按契约写 1080p60。主机选择了超带宽模式时可能没有可用画面。 | 决定 reset 是否恢复板级 preset，以及 1080p30 在网页上的标识。 |
| `/api/ws`、MCP 的 paste 和 HID 输入 | README 定义 64 KiB，schema 的 maxLength 按 Unicode 码点计数。当前另外检查 UTF-8 字节长度。schema 接受任意非空布局名和许多没有 HID 用法的 code。sim 可接收 Unicode，实板按目标键盘布局生成按键，布局无法表达的字符整条拒绝。 | 统一长度单位，规定支持的布局、不能映射字符或按键的错误码，并明确滚轮缩放由哪端执行。 |
| `/api/config`、全部远程通道、panel | 设计要求关闭 KVM 后仅保留副屏。mock 仍继续监听，关闭码表没有这一项。当前实现先让关闭操作返回，远程连接收到 4409，网页停止监听，副屏可以重新启用。 | 在网页关闭提示、重新启用入口和 4409 说明中加入总开关。 |
| `/api/usb/apply`、介质、共享盘和文件 | 合法配置可以为空，此时 kvmd 解绑 gadget。已挂介质或导出共享盘时禁用 storage 返回 409 `in_use`。共享盘只规定板端只读，未规定主机写权限，本实现使用只读 LUN2 和只读本地挂载。若有进行中的写入或下载，冲突使用 `share_busy` 或 `in_use`。 | 确定空组合和使用中的 LUN 的行为，明确共享盘写入方及切换次序。为相关操作补充 409 响应。 |
| 板端硬件和系统服务 | mock 没有设备未接、缺 GPIO、configfs 重建失败或系统命令失败的情形。当前 HTTP 设备操作失败使用统一 500 `internal`，截图无可用帧沿用 503 `no_signal`。状态消息的 power/locks 为必填布尔值，读取失败时只能显示 false。 | 增加设备不可用、应用失败的区分，决定实时状态怎样表达未知。Tailscale 异步动作还需要能被网页查询的失败原因。 |
| 文件、终端和品牌 | README 关闭码表缺非法终端 kind 和 PTY/串口打开失败，当前分别用 4400、1011。logo 非 PNG/SVG 返回 415 `unsupported_type`，OpenAPI 未列出。多个端点也未列通用的非法 JSON 或 MIME 错误。 | 补全关闭码和公共错误响应，让网页可以区分参数、权限和设备故障。 |
| `/api/network`、Tailscale | 网络 schema 对 bind、DNS 元素只要求字符串，也未限制 HTTP/HTTPS 使用同一地址端口、证书缺半对等不可应用的设置。板端校验地址及 TLS 后才接受。Tailscale 安装没有约定软件来源，当前使用上游 stable 的 ARM 包和校验值。 | 明确板端可接受的字段与错误路径，确认 Tailscale 安装源及安装失败的反馈。 |
| 应用升级 | 契约定义了任务、SHA-256 和服务器地址，未定义清单、包布局、大小、并发提交与激活方式。暂用按频道命名的 JSON 清单、最大 256 MiB 的 tar、完成后切换 apps/current。进行中的板端安装返回 409，超限返回 413。 | 确认发布端与离线包生成端采用同一格式，并补充响应码。 |

仓库没有发布服务器地址。遥测线格式按简报留在 [kvmd 说明](../ipkvm/README.md)，待上位机格式确定后可单独替换解码器。

## 2026-11-20 (from Claude) Stream resume

Contract change: on connect and on every `{"t":"resume"}`, `/api/stream` immediately sends the current frame regardless of frame_diff (README, mock stream.ts). kvmd must do the same.
