# kvmd

在 Linux/WSL 中使用 Rust stable。初始账号为 `admin`，密码为 `admin`，首次登录返回 `must_change_password`。在仓库根目录运行以下命令，HTTP 监听 127.0.0.1:8800。

```sh
. "$HOME/.cargo/env"
export CARGO_TARGET_DIR="$HOME/work/KVMPanel/build/kvmd-cargo"
cargo run --manifest-path software/app/ipkvm/Cargo.toml -- --sim \
  --listen 127.0.0.1:8800 --data-dir "$HOME/work/KVMPanel/build/kvmd-sim" \
  --web-dir software/app/web/dist
```

默认板端数据目录为 `/data/kvmd/`。账号密码使用 SHA-256 预散列以保留长密码的所有字节，随后使用独立盐值做 bcrypt。配置和账号通过同一目录的临时文件、fsync 和 rename 持久化。会话只保存在进程内。构建时从相邻的 API 目录读取 OpenAPI 和消息 schema，板上无需安装 YAML 文件。

## 副屏与遥测

sim 同时监听 TCP `127.0.0.1:7701`，板上监听 Unix socket `/run/kvmd/panel.sock`。可用 `--panel` 或 `KVMPANEL_PANEL_SOCK` 指定地址。TCP panel 信任本机进程，仅接受回环监听地址。
临时上位机协议使用 HID 厂商 Output report（EP0 SET_REPORT）。每份报告恰好 64 字节。首字节为报告 ID `0x03`，余下 63 字节为 ASCII `指标ID=值`，尾部补零。例如 `cpu.tctl=64.25` 或 `mem.dimm.a1=null`。指标 ID、单位与量程沿用副屏契约，数值必须有限。未知 ID、坏包、非零填充会被丢弃。每份有效报告更新一项，副屏每秒收到一次汇总。三秒没有有效报告即判定上位机离线，恢复后从新收到的样本开始。正式上位机协议确定后可替换 `panel::metrics::ReportDecoder`。PC 模拟器直接生成指标。

真机约每 8 ms 轮询非阻塞 hidg。Linux f_hid 的 EP0 SET_REPORT 只保留最新的一份，临时上位机应将连续报告错开（例如每 25 ms 发送一项，循环刷新完整一组）；最终吞吐量和丢包率待实板确认。背光每秒根据 BH1750 的 lux 计算，0–1000 lux 以对数曲线映射到 10–100%，读不到传感器时使用手动亮度。关闭自动调节后直接应用手动百分比。上位机离线超过 `offline_sleep_min` 后写 0，设为 0 分钟可禁止自动息屏。恢复通信后恢复当前亮度，仅亮度变化时写 sysfs。
