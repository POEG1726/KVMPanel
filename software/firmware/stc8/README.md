# STC8 电源管理

目标芯片为 STC8G1K08A-36I-DFN8，8 KiB 程序 Flash、4 KiB Data Flash、256 B 内部 RAM 和 1 KiB 片内 XRAM。固件和 SDCC 构建入口在本目录。T113 的串口和升级程序随 Buildroot 构建。 [08A 选型表](https://www.stcmicro.com/cn/stc/stc8g1k08a.html) · [T113 配套源码](../../os/buildroot/package/kvmpanel-stc-power/)

## 固件行为

- SDCC 启动钩子先锁存 P5.4=1、P5.5=0、P3.3=0，再配置推挽输出。P5.4 为高时 Q3 拉低 T113 RESET。P5.5 为高时 Q4/Q10 打开 VCC-PD，P3.3 为高时驱动主电源使能。P3.2 为 ADC2 模拟输入。P3.0 为 UART RX 输入，关闭数字内部上拉。P3.1 为 UART TX。
- ADC 一直开启并逐次重启转换。状态机每 5 ms 取最新样本，使用四分之一权重 IIR、两个窗口和迟滞。沿用按键的 100 nF 硬件消抖，固件无消抖计时。滤波值确认双键后，从原始样本进入窗口的时刻计算按住时长。原始样本离开双键迟滞窗口即取消长按。FUNC 和单键电平只用于退出双键状态，STC 不上报按键事件。
- 关机后要观察到全松开才接受新的双键短按。短按窗口暂定 50–500 ms，完整松开后才开机。持续双键达到 10 s 时优先拉低主电源，随即拉低 T113 RESET、关闭 PD 域，再排队保存 `power_flag=0`。松手只重新允许识别下一次手势。
- 开机先保持复位，开主电源后 20 ms 开 PD，最早 150 ms 释放 RESET。状态机保证发出 PD 使能后再等至少 130 ms。断电后的放电等待和恢复标志为 1 时的冷启动等待暂定 500 ms。20、150 和 500 ms 都需要根据实测复核。
- Data Flash 地址 `0x0000–0x03ff` 使用两个 512 B 扇区。每条记录含一个 0/1 标志、16 位序号、互补字节、CRC 和最后写入的提交字节。换扇区时保留最新有效记录所在扇区。开主轨后保存 1，正常关机在保存 0 后关轨，强关在关轨后保存 0。意外掉电时沿用最后提交的值。空白或无有效记录时保持关机。按键长按进行中延后扇区擦除，字节写入分散到状态机循环。
- 固件 1.1 在配置系统时钟后启动 STC 硬件看门狗，`WDT_PS=2` 对应 8 分频，标称超时约 1.71 s。主循环完成一个新 5 ms 节拍的状态机和 Flash 处理后喂狗。超时复位会重新走安全引脚初始化和 `power_flag` 恢复流程，因此运行中触发时 T113 可能经历一次断电重启。ISP 硬件选项仍关闭上电自动启动看门狗，由应用固件启动。受控超时后的整板恢复和软件复位进入 ISP 都待实测。 [STC8G 手册 §6.2.1](https://www.stcmicro.com/datasheet/STC8G-en.pdf)

## 时钟和 IAP

使用 ISP 设置的 11.0592 MHz HIRC，`CLKDIV=6` 后 `SYSCLK=1.8432 MHz`。08A 有两个 16 位定时器。Timer 0、Timer 1 均用 1T 模式 0 自动重载。Timer 0 每次计数 9216 个周期，重装值 `0xDC00`，得到 5 ms。UART1 模式 1 使用 Timer 1，每次计数 48 个周期，重装值 `0xFFD0`，波特率为 `1,843,200 / (48 × 4) = 9600`。两项分频的量化误差均为 0，波特率的相对误差等于 HIRC 频差。08A 手册给出 25 °C 时 ±0.3%，即标称 9600 bps 时约 ±29 bps，并给出 −40 至 +85 °C 时 −1.38% 至 +1.42% 的频率漂移。板上的校准结果和温漂待实测。

`ADCCFG=0x2F` 选择右对齐 10 位结果，ADC 时钟为 `1.8432 MHz / 2 / 16 = 57.6 kHz`。`ADCTIM=0x3F` 选择 1 个 setup、2 个 hold 和 32 个采样周期。IAP 以 Data Flash 零基地址操作，`IAP_TPS=round(1.8432)=2`，触发字节为 `0x5A`、`0xA5`。原厂 ISP 入口为 `IAP_CONTR=0x60`。 [STC8G 手册 §6、§8.3、§12.5.14、§15、§16、附录 M](https://www.stcmicro.com/datasheet/STC8G-en.pdf)

## 板级接线

2026-11-22 版中，U25 的 STC_5V 通过 R34 与板上 5 V 相连，也引到 H5.4 和 H7.2。PB0/PB1 经 D2/D1 连至 STC UART，H5 引出 STC 端的 TX/RX。H7.1 的 KEY_ADC 连 STC P3.2，并经 Q1 转接 T113-i GPADC0。U25 还连接主电源使能、T113-i 复位和 VCC-PD 开关电路。Power 页标注首烧时暂不装 R34、D2、R32。 [硬件归档](../../../hardware/README.md)

- 引脚分配 CSV 的 `KEY_ADC` 和 `T113_ADC` 行已记录按键电阻及各组合的理论电压。`STC_5V=5.000 V` 时，全松开为 1.307 V，BCLK+ 与 BCLK− 同按为 0.700 V，分别约为 STC 10 位 ADC 的 268 和 143 计数。首次上板分别采集关机和运行时各组合的计数范围，再设置 `KEY_BOTH_LOW/HIGH`、`KEY_IDLE_LOW/HIGH` 和 `KEY_HYSTERESIS`，并确认单键或 FUNC 不会触发双键手势。默认镜像暂禁用按键识别。T113 的 GPADC0 以 AVCC 为满量程，运行时单独测量并设置板级 DT 的 `adc-keys` 阈值。 [引脚分配 CSV](../../../hardware/T113_signal_allocation.csv) · [STC8G 手册 §16.2.3](https://www.stcmicro.com/datasheet/STC8G-en.pdf)
- 正常串口使用 R31/R32 的 3.3 V 外部上拉。实板在 D1/D2 两侧核对电平和 `9600 8N1` 位宽，再用 T113 的 `kvmpanel-stc-power status` 验证收发。首烧与救砖用 H5 外部串口，数据线、地和供电按板级首烧接线与原厂下载器说明确认。用原厂工具写 `kvmpanel-stc8.ihx`，设置 11.0592 MHz IRC、P5.4 普通 IO、默认 UART1 引脚，关闭上电自动看门狗和下载擦除 EEPROM。
- 示波器应确认 VCC-RTC 不迟于其他电源，VCC-IO 先于 VDD-SYS/VDD-CPU 至少 2 ms，VCC-LVDS 先于 VCC-PD，释放 RESET 前所需电源已稳定超过 64 ms。PHY 的 25 MHz 参考时钟、PC1 的 PHY_RESET_N 和 RGMII 时序仍需独立验证。 [T113-i 数据手册 §5.14](../../../hardware/reference/datasheets/T113-i_Datasheet_V1.4.pdf) · [板级适配说明](../../../docs/linux-6.18.md)

## 构建与使用

在有 SDCC、makebin 和 C 编译器的 WSL 环境运行。阈值可写入 `board.h`，也可以通过 Make 的 `BOARD_CPPFLAGS` 同时传入五个 `KEY_*` 宏。

```bash
cd /mnt/d/MyProj/KVMPanel
make -C software/firmware/stc8 all test
```

`build/stc8/` 中包含首烧用的 `.ihx`、在线升级用的 `.bin` 和内存报告 `.mem`。有固件改版时更新 `protocol.h` 里的固件版本。T113 配套使用补丁 0008 和板级 DT，UART2 为 `/dev/ttyS2`，UART0 继续作为控制台。Buildroot 的 `kvmpanel_defconfig` 启用 `kvmpanel-stc-power` 和相关补丁。 [T113 内核和 Buildroot 构建](../../../docs/development.md)

```bash
# Buildroot 已载入 kvmpanel_defconfig 时
make -C "$buildroot" O="$bout" busybox kvmpanel-stc-power

# 在已接线的 T113 上
kvmpanel-stc-power status
poweroff
```

正常 `poweroff` 由 PID 1 请求 STC 等待，执行服务清理、swapoff、卸载或只读重挂载，再核对剩余挂载并通知 STC 就绪。内核执行完设备及 syscore 关闭后，从 UART2 发送最终断电命令。STC 保存 0 后断电。Linux 未完成确认、UART 无响应或 STC 写 Flash 失败时，主轨保持供电，校准后的双键强关仍可用。正常 reboot 不发断电命令。

## 在线升级

PB10/F17 是计划中的 UPDATE_HOLD，输出只接 3.3 V HOLD。完整的外部电路必须同时保持 EN_MAIN、保持 PD 和屏蔽 STC 的 Q3 对 T113 RESET 的作用，并提供释放时的确定电平。当前 PCB 的保持电路尚待加回。首次烧录和坏固件救砖通过外部串口。

```bash
# 仅在确认三个 HOLD 路径可用之后，在 T113 上执行
kvmpanel-stc-power update --hold-wired /path/to/kvmpanel-stc8.bin
```

工具会先提示升级期间暂停 10 s 强关，然后持续持有 PB10 的 GPIO 文件描述符和同一个独占 UART。它协商约 1500 ms 后进入原厂 ISP，在 2400 8E1 等握手，再用 57600 8E1 写入。下载器复用 [IOsetting/stc8prog](https://github.com/IOsetting/stc8prog/tree/5151cba878b99024a8b4080d9a115ff03f09b74b) 的原厂 ISP 传输实现，检查 08A 型号、EEPROM 保留选项、P5.4 普通 IO 和 IRC 频率，使用仅擦除程序区的命令，保留 Data Flash 与原有选项。升级程序和正常关机用同一把锁，升级中 PID 1 拒绝普通关停和重启。

ROM 退出后，工具切回 9600 8N1，用新的请求确认镜像版本、持久化标志仍为 1、STC 已驱动主轨和 PD 并释放复位，随后才拉低 HOLD。出错时原进程继续持有 HOLD，可按提示向该 PID 发 `SIGUSR1` 重试同一镜像。进程被强制杀死、T113 崩溃或掉电时，GPIO 控制权随内核释放，恢复依赖外部接线和原厂下载入口。

## 离线验证

- SDCC 4.2.0 默认镜像使用 5889 B 程序 Flash。启用模拟 ADC 窗口的独立构建使用 6036 B。两者的栈从内部 RAM `0x66` 开始，链接后有 154 B 栈空间，XRAM 用量为 0。编译生成的汇编已核对三个锁存值先于模式寄存器写入，以及看门狗启动时机和喂狗位置。栈空间来自链接报告。
- `make test` 通过按键、一次长按强关后松手保持关闭、再次短按、强关时无 Linux ACK、上电顺序、正常关机、ISP 请求后暂停计时，以及 Data Flash 0/1 恢复、每个提交步骤中断和部分擦除测试。
- T113 的完整 Buildroot `rootfs.cpio.gz` 已构建，归档内包含修改后的 BusyBox 和静态 ARM 升级程序，QEMU ARM 验证了 CLI 入口。伪终端验证了应用串口命令、旧 nonce 和坏 CRC 丢弃及接收超时。主机端以模拟 ROM 运行实际更新程序和复用的 ISP 传输代码，覆盖代码区擦除、写入 ACK 丢失、擦除选项冲突、版本错误、输出未接管及成功释放 HOLD。模拟中 Data Flash 保持原值，物理保留效果需要按目标 boot ROM 版本实测。
- Linux 6.18.64 的 `zImage` 已构建并核对了串口关机回调、GPADC 和 `adc-keys` 的链接符号。基础、DSI、组合设备树已编译并核对 UART2 和 GPADC0。实板仍需核对 ADC 量程与窗口、UART 电平与频差、上电和断电时序、PB10 的 HOLD 接线，以及真实 ISP 下的标志保留和输出接管。

Buildroot 解包并应用 ISP 补丁后，可在主机复跑模拟升级。以下命令在仓库根目录执行，`$bout` 是上面的 Buildroot 输出目录。

```bash
pkg=software/os/buildroot/package/kvmpanel-stc-power
isp=("$bout"/build/kvmpanel-stc-power-*/src)
cc -Isoftware/firmware/stc8 -I"${isp[0]}" -pthread -Wl,--wrap=ioctl \
    "$pkg/test_update.c" "$pkg/isp.c" "$pkg/update.c" \
    "${isp[0]}/stc8prog.c" "${isp[0]}/stc8db.c" -o build/stc8/test_update
build/stc8/test_update
```
