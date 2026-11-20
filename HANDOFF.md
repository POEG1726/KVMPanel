# 当前交接

更新日期：2026-11-20。项目路径：`D:\MyProj\KVMPanel`。

## 已确认决策

- 主功能：IP KVM、PC 机箱 Dashboard 副屏。
- T113-i，目标 Linux 6.18 LTS，普通非 PREEMPT_RT 配置。
- 上游源码固定到 `manifests/sources.lock.json`；U-Boot 为评估基线，不能据版本号认定支持当前 DDR 和 Flash 启动。
- Buildroot 使用外部工程目录；厂商 Tina 仅作参考，公开框架/overlay 不等于完整 SDK。

## 当前完成范围

- 建立目录、仓库、源码锁定与管理工具。
- 导入此前 6.18.64 静态源码审查和当前 PCB 器件快照。
- 上游下载、完整性检查和 WSL 工作目录状态，见 `provenance/bootstrap-status.json`。
- 硬件资料导入与原生 EDA 导出状态，见 `hardware/README.md` 和 `provenance/imports.json`。

## 尚未完成

- 没有经过验证的板级 DTS、内核 defconfig、U-Boot 配置或系统镜像。
- 没有应用代码、STC 固件或已验证的 MCU 关机协议。
- 完整 Tina SDK 缺失；应取得同一版本的 kernel、brandy/DDR、platform、prebuilt 等包，先核对来源和哈希。
- Ubuntu WSL 现有环境缺 make、GCC/G++、bison、flex 等编译依赖，当前尚不能编译；未安装系统软件包。
- 未执行硬件读写、上电、Reset、烧录、DDR 稳定性测试或视频性能测试。

## 下一阶段

1. 取得当前原理图/网表和可恢复的原生 EDA 工程归档；以最新接线重新整理管脚与电源域表。
2. 确认屏幕/触摸具体型号、初始化资料；明确 IP KVM 的分辨率、帧率、延迟和编码格式验收条件。
3. 固定启动链与 DDR 参数，先让 6.18 进入串口控制台，核对内存和两核上线。
4. 建立板级 DTS 和 Buildroot defconfig，验证存储、网络、USB HID、Front Panel、STC 关机。
5. 适配显示和触摸；随后攻克 LT8619C + CSI 与 JPEG 编码。

详细分阶段验收见 `docs/bringup/plan.md`。新接手者先查看 `git status`，不要直接运行任何烧录或远程电源命令。
