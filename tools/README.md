# 开发工具

- `sources.py fetch`：按锁文件下载完整源码快照并验证；不下载所有历史。
- `sources.py verify --full`：复核本地 mirror 的固定提交、来源、必要文件和 Git 对象。
- `prepare-workspaces.py`：在 Linux/WSL 原生文件系统创建独立完整工作目录，不覆盖已有改动。

本目录工具不执行构建、烧录、硬件读写或 PC 电源控制。运行位置和命令见 `docs/bringup/environment.md`。
