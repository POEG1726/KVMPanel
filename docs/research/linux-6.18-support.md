# T113-i / Linux 6.18 适配检查

日期：2026-11-20。检查基线：v6.18 与 v6.18.64，后者提交 `fd7cfd39c88596fec816639ad83cbf3e9d316372`。

这是静态源码检查。没有编译验证、实板启动或性能测试；T113-S 的支持不能直接当作 T113-i 全功能验证。

## 结果

| 功能 | 检查结果 | 后续 |
|---|---|---|
| 板级 DTS | 无 T113-i 自定义板配置；存在 T113-S / D1 共用基础 | 核对差异，建立本板 DTS |
| DDR / 双核 | DDR 属于启动程序；Linux 有 PSCI 路径，U-Boot 有 R528 实现 | 固定 DDR 参数与启动固件，实测两核 |
| UART/GPIO/I²C/SPI/USB | 有共享节点与驱动 | 填本板 pinctrl、供电和角色 |
| RTL8211F / W25N04KW / BH1750 | 有相应驱动条目 | 配置并验证；不代表 Flash 启动链已兼容 |
| DSI | 已有 DE/TCON/DSI/D-PHY；未找到 TD4322 命名面板驱动 | DRM panel 适配；触摸协议另确认 |
| CSI | sun6i-csi 无 T113/D1 匹配项，共用 DTS 无 CSI 节点 | 对照 Tina VIN 的寄存器、DMA、IRQ；不是只加 compatible |
| LT8619C | 原版树无驱动 | 参考 Rockchip GPL 驱动，剥离私有依赖，接 V4L2 |
| JPEG/MJPEG 编码 | Cedrus 原版接口为解码 | 评估社区 V4L2 编码补丁，再考虑 Tina 编码 ABI |
| 温度 / 重启 | THS 驱动有 D1 支持，T113 DTS 集成缺失；watchdog reserved | 评估后续上游 DT 补丁、固件所有权与校准 |
| 自身关机 | STC 是板级自有协议 | 小型协议/关机驱动或合适的最终断电信号 |

6.18.64 已含部分 DSI/LVDS mux 修复；DSI mod clock 仍在 probe 阶段获取，需评估后续 bind-time 修复。不能把 6.18 基础版和最新 6.18.y 的缺陷一概而论。

## 移植策略

1. 基础外设复用主线，主要改 DTS/config。
2. 面板只迁移命令、时序和电源顺序，保留主线 DRM 框架。
3. LT8619C 复用寄存器和检测逻辑；T113 CSI 按数据手册/Tina 寄存器实现必要的 V4L2 驱动适配。
4. 编码优先评估标准 V4L2 路线。社区作者报告 T113 JPEG 测试成功不等于当前 T113-i + 6.18 已验证。
5. STC、面板电源等板级特有功能保持独立小模块，不整体搬入厂商显示/VIN 框架。

## 证据

- `provenance/research-20261120/source_audit.json`：文件位置、检查模式、版本和解释。
- `hardware/reference/board-components-20261120.json`：当前 PCB 器件清单，只证明器件属性。
- https://github.com/gregkh/linux/tree/v6.18.64
- https://github.com/u-boot/u-boot/blob/v2025.10/arch/arm/cpu/armv7/sunxi/psci.c （前次审查的启动参考版本；本项目下载 v2026.10，后续需重新核对）
- https://github.com/DongshanPI/T113i_DevKitF_Tina5SDK
- https://github.com/rockchip-linux/kernel/blob/develop-6.1/drivers/media/i2c/lt8619c.c
- https://forum.armbian.com/topic/26551-video-encoder-using-mainline-kernel/
