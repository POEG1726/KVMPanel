# 主机端 HID 通道

约定依据见 [MCP 设计第 9 节及第 16 节](../../../docs/superpowers/specs/2026-11-23-mcp-design.md#9-主机端程序通道)。同一厂商 HID 集合可位于独立、绝对鼠标或合并鼠标接口。复用已有 IN 端点和 EP0 SET_REPORT。

| 方向 | 报告 | 负载 |
| --- | --- | --- |
| 板 → PC | Input 4 | `?<seq>:<method>` 请求，或 `!<seq>:<part>` 分片应答 |
| PC → 板 | Output 3 | `<<seq>:<part>/<total>:<data>` 回复，`^caps=<methods>` 能力声明，或原有遥测包 |

报告共 64 字节，首字节是报告 ID，随后最多 63 字节负载，未用部分补零，满包无需终止符。前缀为 ASCII。`seq` 是 0–65535 的无符号十进制循环序号，`part` 从 1 开始，`total` 是总片数。第一版方法 `info`、`clipboard`、`eject` 均无参数。`eject` 固定指同一 USB 复合设备的 LUN 2。

每条 USB 链路同一时刻只处理一个请求。PC 把序列化后的 UTF-8 JSON 按字节切片填入 `data`，可以跨 UTF-8 字符边界。板端先解析头部并拼接原始字节，接收每片后回复 ACK，包括末片。PC 收到匹配 `seq` 和 `part` 的 ACK 才发送下一片，末片收到 ACK 后恢复遥测。两端忽略无关序号及失序包；板端可对最近已接收的重复片重发 ACK。PC 的 Output 3 只有一个写入者，回复期间挂起心跳、能力及其他遥测包，恢复时优先发送到期的心跳和能力。两端等待下一片或 ACK 最多 2 秒，整条事务最多 10 秒。USB 重新枚举会结束在途事务。

完整 JSON 最多 16,384 字节，包括 JSON 结构和转义。回复是一个对象，成功时为 `{"ok":true,"data":{...}}`，失败时为 `{"ok":false,"error":"snake_case"}`。

- `info` 的 `data` 含 `foreground`、`cursor`、`displays`、`session`。`foreground` 为 `{title,process}` 或 `null`，无法读取单个字段时该字段为 `null`。`cursor` 为采集显示器上的 `{x,y}`（0–1），在画面外或不可读取时为 `null`。`displays` 是 `{id,name,x,y,width,height,scale,primary,captured}` 数组，位置和分辨率用物理像素，`scale` 为比例（100% 为 `1.0`），不可用时为 `null`。`session` 取 `unlocked`、`locked`、`secure_desktop` 或 `screensaver`。可选 `truncated:true` 表示过长内容已缩减。
- `clipboard` 的 `data` 为 `{text,truncated}`，在有效 UTF-8 边界裁剪到完整 JSON 能装入上限。没有文本时返回 `text:""`。开关关闭或当前程序不在已解锁的活动本地会话时返回 `host_feature_disabled`。
- `eject` 成功时 `data` 为 `{}`。设备无法唯一确认、权限不足、卷忙或系统拒绝时返回相应失败原因。板端收到失败时保留共享盘归属，再按 MCP 规则处理确认令牌。

能力声明如 `^caps=info,eject`，启用剪贴板后为 `^caps=info,clipboard,eject`。每次按完整集合替换，旧版 `kvmd` 会忽略。PC 在连接后、能力变更后和每 5 秒发送，优先先发既有 `!<16hex>:<interval>` 心跳。板端只有发现能力声明才把新版主机程序视为可用，随后用有效心跳、能力声明和回复维持在线状态。使用现有心跳会话 ID 区分启动前后两次程序会话。回复时用独立的主机在线状态，遥测沿用现有新鲜度规则。
