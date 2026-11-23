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

正式 Windows 上位机的设备发现、指标清单和实施顺序见 [遥测采集计划](../../../docs/superpowers/specs/2026-11-23-pc-telemetry-design.md)。

sim 同时监听 TCP `127.0.0.1:7701`，板上监听 Unix socket `/run/kvmd/panel.sock`。可用 `--panel` 或 `KVMPANEL_PANEL_SOCK` 指定地址。TCP panel 信任本机进程，仅接受回环监听地址。

临时上位机协议使用 HID 厂商 Output report（EP0 SET_REPORT）。每份报告恰好 64 字节。首字节为报告 ID `0x03`，余下 63 字节为 ASCII `指标ID=值`，尾部补零。例如 `cpu.tctl=64.25` 或 `mem.dimm.a1=null`。指标 ID、单位与量程沿用副屏契约，数值必须有限。未知 ID、坏包、非零填充会被丢弃。每份有效报告更新一项，副屏每秒收到一次汇总，只含上位机报告过的指标。本秒没有更新的已知指标取 null。槽位名取自 `mem.dimm.<槽位>`，临时解码器每次连接最多保留 256 个不同槽位名。模拟器仅报告 A2、B2。三秒没有有效报告即判定上位机离线，恢复后从新收到的样本开始。正式上位机协议确定后可替换 `panel::metrics::ReportDecoder`。PC 模拟器直接生成指标。

真机约每 8 ms 轮询非阻塞 hidg。Linux f_hid 的 EP0 SET_REPORT 只保留最新的一份，临时上位机应将连续报告错开（例如每 25 ms 发送一项，循环刷新完整一组）。最终吞吐量和丢包率待实板确认。背光每秒根据 BH1750 的 lux 计算，0–1000 lux 以对数曲线映射到 10–100%，读不到传感器时使用手动亮度。关闭自动调节后直接应用手动百分比。上位机离线超过 `offline_sleep_min` 后写 0，设为 0 分钟可禁止自动息屏。恢复通信后恢复当前亮度，仅亮度变化时写 sysfs。

## 板端接口

| 功能 | 实现和部署参数 |
|---|---|
| 视频 | 用 `VIDIOC_QUERY_DV_TIMINGS` 查询 LT8619C 时序，`media-ctl` 配置 CSI，四个 mmap 缓冲采集 YU12，libjpeg-turbo 直接编码三个平面。EDID reset 恢复板级默认值，首选 1080p30，并提供 720p60 和 VGA。最后一个订阅者离开后停止采集。设备路径分别由 `KVMD_HDMI_DEVICE`、`KVMD_MEDIA_DEVICE`、`KVMD_VIDEO_DEVICE` 设置，默认是 `/dev/v4l-subdev0`、`/dev/media0`、`/dev/video0`。 |
| USB | 枚举时从 configfs 读取设备号，打开并复用 hidg 描述符，重建或显式重置时刷新。键盘灯和上位机遥测使用 HID Output report。应用 USB 设置时调用镜像内的 `/usr/sbin/kvmpanel-gadget`。开机恢复最近一次 apply 的组合。 |
| 主板控制 | 使用 libgpiod 1.x 的 `gpioget`、`gpioset`。板级部署用 `KVMD_GPIO_CHIP` 选择主 PIO（默认 gpiochip0）。按新版接线，`KVMD_GPIO_POWER=141`（PE13）、`KVMD_GPIO_RESET=131`（PE3）、`KVMD_GPIO_POWER_LED=140`（PE12）均为高有效。电源状态接口读取 PE12，返回 `{"on":true}` 或 `{"on":false}`。HDD 灯在 H1/H3 间直通。 |
| 显示和传感器 | 自动查找 bh1750 IIO 和 cpu-thermal 区。背光从 max_brightness 换算，存在多个设备时用 `KVMD_BACKLIGHT` 指定 sysfs 目录。 |
| 文件和介质 | `KVMD_SD_ROOT=/mnt/sd`、`KVMD_SHARE_ROOT=/mnt/share`、`KVMD_SYSTEM_ROOT=/`。共享盘需要事先将 `KVMD_SHARE_IMAGE`（默认 SD 根目录的 share.img）挂成独立 loop 文件系统。导出时将冻结的镜像复制到同一文件系统，板端挂载这份只读快照，主机通过 LUN2 读写原镜像。导出需要约一份镜像大小的额外空间。主机刷新写缓存并卸载后，可收回共享盘。本地重新挂载原盘，主机写入的内容此时可见。存在活跃写入时切换返回 409 `share_busy`。 |
| HTTP/HTTPS | 板端读取已保存的 bind、端口和证书。首次开启 HTTPS 且未指定证书时在数据目录 tls/ 下生成证书对。显式 `--listen` 选择固定 HTTP 地址。关闭 KVM 后通过 panel socket 重新启用。 |

多个控制台共用实板键鼠。最后一个可输入会话退出或转为只读时释放已按下的键和鼠标按钮。两个模式都按配置的 US、UK 或 DE 布局校验整条粘贴，UTF-8 编码上限为 64 KiB。任一字符不可映射时，在发送第一键前返回 `unmappable`。滚轮反向与倍率由客户端换算。防休眠抖动每 30 秒检查一次，拖动期间跳过。

模拟上传和文件操作实际写入数据目录。模拟 URL 下载生成固定 64 MiB 数据，可离线验证进度、暂停、取消、空间和校验和。下载任务保留最近完成的 20 项，真机下载的单次读超时为 60 秒。模拟终端运行当前 Linux 用户的真实 shell。模拟串口使用独立 PTY。

## 应用升级

板端暂从 `<server>/<channel>.json` 读取 `version`、`package`、`sha256`。`package` 可以是相对清单的 URL。离线包通过 `X-Sha256` 校验。当前包为不压缩的 tar，包含 kvmd、panel、web/index.html 和网页资源，最大 256 MiB。后端在数据目录暂存，校验完成后原子切换 apps/current，重启服务。

基础镜像内的 `S95kvmd` 调用同一镜像的 `/usr/sbin/kvmd --verify-update` 监督候选进程。它先验证 panel 由候选程序提供，启用网页时还检查实际 HTTP 端口的 `/api/health`。60 秒内未通过即恢复上一个应用版本并重新启动，首次升级失败时恢复镜像内置程序。启动脚本在执行候选程序前记录试用期限。若验收中途整板重启，下次开机会先恢复旧版。试运行结束后保留当前版本和上一版。部署这项保护需要更新基础镜像中的启动脚本和验证程序。[构建与 QEMU 验证](../../../docs/development.md)

板端启动后读取已保存的主机名、SSH、mDNS 和看门狗设置。SSH 主机密钥位于数据分区。模拟环境的这些设置仅供客户端联调。首次上板需复核看门狗、主 PIO 的 gpiochip 编号及控制信号电平。
