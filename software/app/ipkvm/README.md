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

[Windows 采集器](../pc-telemetry/README.md) 沿用 HID Output report 3。支持批量 `id=value`、会话心跳和设备名，也兼容原有单项报告。新版会话按采样周期判断缺样，支持低频采集、多个 GPU、磁盘和网卡。动态设备指标通过现有 panel socket 的 `telemetry` 消息上报。

`board` 每秒上报板子自身状态，PC 遥测离线时也继续发送。`pc_power_on` 读取 PE12 后才短按 PE13，电源灯亮或刚发过开机脉冲时返回 `pc_on`，读不到灯时返回 `device_unavailable`。副屏用 `metric_catalogue` 获取已知字段，包括隐藏项。 [副屏程序](../dashboard/README.md)

sim 同时监听 TCP `127.0.0.1:7701`，板上监听 Unix socket `/run/kvmd/panel.sock`。可用 `--panel` 或 `KVMPANEL_PANEL_SOCK` 指定地址。TCP panel 信任本机进程，仅接受回环监听地址。

临时上位机协议使用 HID 厂商 Output report（EP0 SET_REPORT）。每份报告恰好 64 字节。首字节为报告 ID `0x03`，余下 63 字节为 ASCII `指标ID=值`，尾部补零。例如 `cpu.tctl=64.25` 或 `mem.dimm.a1=null`。指标 ID、单位与量程沿用副屏契约，数值必须有限。未知 ID、坏包、非零填充会被丢弃。每份有效报告更新一项，副屏每秒收到一次汇总，只含上位机报告过的指标。本秒没有更新的已知指标取 null。槽位名取自 `mem.dimm.<槽位>`，临时解码器每次连接最多保留 256 个不同槽位名。模拟器仅报告 A2、B2。三秒没有有效报告即判定上位机离线，恢复后从新收到的样本开始。正式上位机协议确定后可替换 `panel::metrics::ReportDecoder`。PC 模拟器直接生成指标。

真机约每 8 ms 轮询非阻塞 hidg。Linux f_hid 的 EP0 SET_REPORT 只保留最新的一份，临时上位机应将连续报告错开（例如每 25 ms 发送一项，循环刷新完整一组）。最终吞吐量和丢包率待实板确认。背光每秒根据 BH1750 的 lux 计算，0–1000 lux 以对数曲线映射到 10–100%，读不到传感器时使用手动亮度。关闭自动调节后直接应用手动百分比。上位机离线超过 `offline_sleep_min` 后写 0，设为 0 分钟可禁止自动息屏。恢复通信后恢复当前亮度，仅亮度变化时写 sysfs。

## 板端接口

| 功能 | 实现和部署参数 |
|---|---|
| 视频 | `kvmd` 查询 LT8619C 时序并通过 CSI 采集 YU12。发现 `cedrus-enc` 时将 YU12 整理成 NV12，交给 T113 VE 编码。节点缺失、格式不支持或编码失败时用 libjpeg-turbo。编码复用输入和输出 mmap 缓冲，画质变化时重新配置。EDID reset 恢复板级默认值，首选 1080p30，并提供 720p60 和 VGA。最后一个订阅者离开后释放采集和编码。 |
| USB | 枚举时从 configfs 读取设备号，打开并复用 hidg 描述符，重建或显式重置时刷新。键盘灯和上位机遥测使用 HID Output report。应用 USB 设置时调用镜像内的 `/usr/sbin/kvmpanel-gadget`。开机恢复最近一次 apply 的组合。 |
| 主板控制 | 使用 libgpiod 1.x 的 `gpioget`、`gpioset`。板级部署用 `KVMD_GPIO_CHIP` 选择主 PIO（默认 gpiochip0）。按新版接线，`KVMD_GPIO_POWER=141`（PE13）、`KVMD_GPIO_RESET=131`（PE3）、`KVMD_GPIO_POWER_LED=140`（PE12）均为高有效。电源状态接口读取 PE12，返回 `{"on":true}` 或 `{"on":false}`。HDD 灯在 H1/H3 间直通。 |
| 显示和传感器 | 自动查找 bh1750 IIO 和 cpu-thermal 区。背光从 max_brightness 换算，存在多个设备时用 `KVMD_BACKLIGHT` 指定 sysfs 目录。 |
| 文件和介质 | `KVMD_SD_ROOT=/mnt/sd`、`KVMD_SHARE_ROOT=/mnt/share`、`KVMD_SYSTEM_ROOT=/`。共享盘需要事先将 `KVMD_SHARE_IMAGE`（默认 SD 根目录的 share.img）挂成独立 loop 文件系统。导出时将冻结的镜像复制到同一文件系统，板端挂载这份只读快照，主机通过 LUN2 读写原镜像。导出需要约一份镜像大小的额外空间。主机刷新写缓存并卸载后，可收回共享盘。本地重新挂载原盘，主机写入的内容此时可见。存在活跃写入时切换返回 409 `share_busy`。 |
| HTTP/HTTPS | 板端读取已保存的 bind、端口和证书。首次开启 HTTPS 且未指定证书时在数据目录 tls/ 下生成证书对。显式 `--listen` 选择固定 HTTP 地址。关闭 KVM 后通过 panel socket 重新启用。 |

多个控制台共用实板键鼠。最后一个可输入会话退出或转为只读时释放已按下的键和鼠标按钮。两个模式都按配置的 US、UK 或 DE 布局校验整条粘贴，UTF-8 编码上限为 64 KiB。任一字符不可映射时，在发送第一键前返回 `unmappable`。滚轮反向与倍率由客户端换算。防休眠抖动每 30 秒检查一次，拖动期间跳过。

模拟上传和文件操作实际写入数据目录。模拟 URL 下载生成固定 64 MiB 数据，可离线验证进度、暂停、取消、空间和校验和。下载任务保留最近完成的 20 项，真机下载的单次读超时为 60 秒。模拟终端运行当前 Linux 用户的真实 shell。模拟串口使用独立 PTY。

视频节点按 LT8619C、CSI 和 Cedrus 的设备名发现。可用 `KVMD_HDMI_DEVICE`、`KVMD_MEDIA_DEVICE`、`KVMD_VIDEO_DEVICE` 或 `KVMD_MJPEG_DEVICE` 覆盖节点路径。`KVMD_MJPEG_DEVICE=off` 强制软件编码，可用于实板对照。成功打开硬件编码或发生回退时，`kvmd` 记录日志。 [板级编码测试](../../../docs/linux-6.18.md#jpegmjpeg-编码)

## 机箱物理按键

板端每 20 ms 读取 IIO 设备 `name=sun20i-gpadc` 的 `in_voltage0_raw`。可用 `KVMD_KEY_ADC` 覆盖完整文件路径。启动时找不到节点会记录并跳过按键任务。有节点但后续读取失败时，放弃进行中的手势并保留已学基准，恢复读取后继续工作。`--sim` 只在设置覆盖路径时采样。默认日志在校准、重校准和状态变化时显示原始计数、基准和键态。`KVMD_KEY_ADC_TRACE=1` 另记录每次采样。 [GPADC 驱动与接线](../../../docs/linux-6.18.md#c906hifi4-与-gpadc)

E12 按键板在 5.000 V 下的全松开名义值为 1.649 V。按 GPADC 名义满量程 1.8 V 和 12 位输出换算，首次用 3277–4094 计数（约 1.44–1.799 V）内的三个连续样本校准，后两笔与首笔相差最多 16 计数，约 7 mV。下限低于按 1% 电阻容差、4.75 V 供电、−5% 总增益和 −10 mV 偏移计算的 1.459 V，上限取最高未截幅码。确认全松开后，稳定且与基准相差不超过 3% 的读数以 1/32 权重跟随。符合校准条件且高于旧基准至少 6% 的稳定读数直接重设基准，可纠正启动时误把按住 BCLK+ 的电平当成全松开。

满量程码 4095 按全松开处理，沿用两样本确认和松键动作。它既不参与首次校准，也不改变已有基准。冷启动只有截幅样本时，可以确认全松开，比例判键仍需先取得稳定的未截幅基准。

未截幅时按 `原始读数 / 全松开基准` 从高到低匹配窗口。每个下限计入窗口，边界取 128 组 1% 电阻容差中相邻键态的最近边缘中点。

| 键态 | 5 V 下理论电压（V） | 理论比值 | 窗口下限 |
|---|---:|---:|---:|
| 全松开 | 1.627–1.671 | 1.000000 | 0.928031 |
| BCLK+ | 1.388–1.428 | 0.851052–0.856062 | 0.775632 |
| BCLK− | 1.130–1.166 | 0.691670–0.700211 | 0.624595 |
| BCLK+ 与 BCLK− | 0.896–0.926 | 0.547519–0.557520 | 0.482820 |
| FUNC | 0.670–0.693 | 0.408070–0.418120 | 0.344458 |
| BCLK+ 与 FUNC | 0.448–0.465 | 0.272529–0.280845 | 0.204384 |
| BCLK− 与 FUNC | 0.216–0.225 | 0.131391–0.136240 | 0.065696 |
| 无效（三键或 H7 断开） | 0 | 0 | 0 |

对未截幅的全松开基准，按 4.75–5.25 V 供电、±5% 总增益、±10 mV 共模偏移的端点和 12 位计数量化复算，最小窗口余量约为基准的 5.88%，出现在 BCLK+ 与 FUNC 组合靠近 FUNC 的边界。实板仍需测实际 GPADC 读数、噪声和窗口余量。

连续两次落在同一窗口才确认键态。从全松开进入 FUNC 才开始计时，700 ms 长按执行一次，在低于 700 ms 松开时执行短按。确认组合或无效状态会取消待执行动作。BCLK 单键和组合键只记录。STC 独立管理双键开关机。`pause_kvm` 关闭远程服务，副屏 `Display` 页的 `Remote KVM` 可重新启用。`stop_mcp` 走同一急停入口。 [功能键契约](../api/README.md#副屏-panel-socket) · [STC 正式窗口](../../firmware/stc8/README.md)

原生模拟使用下列入口，Rust 测试另覆盖单样本噪声、按住时长及跨窗口取消。

```sh
# CARGO_TARGET_DIR 指向原生构建，LD_LIBRARY_PATH 指向含 libturbojpeg.so.0 的目录。
PYTHONDONTWRITEBYTECODE=1 python3 software/app/ipkvm/tests/keys_sim.py "$CARGO_TARGET_DIR/debug/kvmd"
```

可写文件模拟验证了 E12 按键七态、无效输入、基准学习与自愈、±5% 增益与 ±10 mV 偏移、两样本确认、截幅松键、FUNC 配置动作、暂停后恢复，以及物理键取消执行中的 MCP 按键。原生 Rust 57 项单元测试和 54 项集成测试通过，含冷启动截幅与已有基准截幅的回归测试。HTTP 22 个 MCP 工具回归通过。WSL Cortex-A7 musl 静态 `kvmd` 去符号后为 13,136,808 B，与 `066b3b2` 相同。实板还需验证实际读数、窗口余量、短长按手感及急停响应。

## MCP 服务

网页 MCP 设置页生成分权限的 Bearer key。客户端连接同一 HTTP/HTTPS 监听上的 `/api/mcp/v1`，使用 Streamable HTTP JSON。管理接口、工具 Schema、权限和现有 REST 兼容入口见 [API 契约](../api/README.md#mcp)。Windows 端与板端使用同一 [HID 协议](../api/host-agent.md)。

截图沿用现有 MJPEG 帧。需要裁剪或缩放时用 libjpeg-turbo 解码，再对输出图像编码。等待工具比较 1/8 分辨率的亮度图。回看有网页订阅时复用视频流，只有回看订阅时约每秒采一帧。带 MCP 的 ARM 构建仍沿用 Bootlin sysroot 的静态 libturbojpeg。运行原生 Linux 模拟器时，截图及帧差需要 `libturbojpeg.so.0`，Ubuntu 24.04 对应的包名为 `libturbojpeg`。

网页和副屏停止入口会撤销 MCP 输入与可取消的电源动作。共享盘和虚拟介质切换会完成当前一致性操作。人工输入优先默认开启，接管前会先释放 MCP 键鼠。有可写控制台会话时，MCP 等待最后一次人工输入后的交还时间，默认两秒，可在网页设为 1–60 秒。关闭优先后，双方可以交错发送 HID 报告，释放 MCP 按键和按钮时保留人工正在按住的键和按钮。MCP 电源脉冲使用可取消的 `gpioset` 子进程，普通网页和副屏电源入口保留现有脉冲行为。

在 WSL 的仓库根目录启动离线验证。脚本会创建临时数据目录和回环监听，通过 HTTP 检查全部工具、确认令牌及急停，并清理模拟进程。

```sh
. "$HOME/.cargo/env"
export CARGO_TARGET_DIR="$HOME/work/KVMPanel/build/kvmd-cargo"
cargo test --locked --manifest-path software/app/ipkvm/Cargo.toml
cargo build --locked --manifest-path software/app/ipkvm/Cargo.toml --bin kvmd
python3 software/app/ipkvm/tests/mcp_http.py "$CARGO_TARGET_DIR/debug/kvmd"
```

2026-11-23 离线结果。原生 Rust 49 个单元测试及 53 个集成测试通过。全部 22 个工具在原生 `--sim` 和 QEMU Cortex-A7 Linux 来宾中的 ARM 静态 `--sim` 通过 HTTP 验证，涵盖可写介质确认和强关途中急停。来宾测试发现 32 位 `openat2` 打开超过 2 GiB 的镜像需要 `O_LARGEFILE`，补齐后约 12.2 GB 的模拟磁盘挂载及确认卸载通过。MCP Inspector CLI 另验证 `tools/list`、截图、点击、开机守护和进 BIOS。

用相同 ARMv7 musl 目标、Cortex-A7 参数和 Bootlin sysroot 对比 `98f4a7f`。去符号的 `kvmd` 从 12,063,656 字节增至 13,030,312 字节，增加 966,656 字节（944 KiB）。ARM 模拟使用同一份去符号可执行文件。实际 HDMI、GPIO、USB HID 和 LED 路径待实板复测。

观察类审计在内存中按密钥和分钟合并，分钟结束或正常退出时写盘。突然断电可能丢失尚未落盘的观察记录。密钥的最近使用时间最多每分钟持久化一次。

## 应用升级

板端暂从 `<server>/<channel>.json` 读取 `version`、`package`、`sha256`。`package` 可以是相对清单的 URL。离线包通过 `X-Sha256` 校验。当前包为不压缩的 tar，包含 kvmd、panel、web/index.html 和网页资源，最大 256 MiB。后端在数据目录暂存，校验完成后原子切换 apps/current，重启服务。

基础镜像内的 `S95kvmd` 调用同一镜像的 `/usr/sbin/kvmd --verify-update` 监督候选进程。它先验证 panel 由候选程序提供，启用网页时还检查实际 HTTP 端口的 `/api/health`。60 秒内未通过即恢复上一个应用版本并重新启动，首次升级失败时恢复镜像内置程序。启动脚本在执行候选程序前记录试用期限。若验收中途整板重启，下次开机会先恢复旧版。试运行结束后保留当前版本和上一版。部署这项保护需要更新基础镜像中的启动脚本和验证程序。[构建与 QEMU 验证](../../../docs/development.md)

板端启动后读取已保存的主机名、SSH、mDNS 和看门狗设置。SSH 主机密钥位于数据分区。模拟环境的这些设置仅供客户端联调。首次上板需复核看门狗、主 PIO 的 gpiochip 编号及控制信号电平。
