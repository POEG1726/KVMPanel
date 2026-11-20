# 系统分层

## 产品功能

两个主要功能是 IP KVM 与 PC 机箱 Dashboard。PC 电源/复位、Front Panel 状态和 BCLK 控制是公共设备服务的附加能力。

```text
浏览器远程客户端                 本地 Dashboard 屏幕
       │                               │
       └──── 应用：IP KVM / Dashboard ──┘
                       │
              公共设备与状态服务
                       │
       Linux 6.18 / V4L2 / DRM / USB Gadget
                       │
        T113-i + LT8619C + PHY + Flash
                       │
          STC 电源管理 / Front Panel
```

## 代码归属

- `software/app/ipkvm/`：视频会话、输入事件和 KVM 功能。传输协议/编码格式尚未冻结。
- `software/app/dashboard/`：本地界面与状态显示；UI 技术栈待资源/性能评估。
- `software/app/services/`：遥测、设备状态、权限和互斥、PC 电源、BCLK、配置与持久化。
- `software/os/`：设备树、驱动补丁、内核/启动程序/根文件系统配置。
- `software/firmware/stc8/`：常供电 MCU 的固件与关机/唤醒协议。

应用使用标准系统接口；精确总线时序、关机末端控制等放在合适的硬件/驱动层。普通用户态 GPIO sleep 不能作为严格微秒脉冲发生器。

## 待冻结的接口

- PC 遥测来源、字段、更新率以及离线显示行为。
- KVM 的视频格式、分辨率/帧率/延迟、USB HID 组合和会话安全。
- MCU 的开机、关机准备、最终断电与故障恢复协议。
- Dashboard 在 KVM 运行时的资源占用、优先级及交互。

T113-i 没有硬件 H.264 编码器。MJPEG 是待验证的可行路线；若 H.264 硬件编码是硬性条件，应先重新审视硬件方案。
