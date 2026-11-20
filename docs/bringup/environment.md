# 开发环境

## 存放位置

- 主仓库：`D:\MyProj\KVMPanel`，WSL 对应 `/mnt/d/MyProj/KVMPanel`。
- 上游 bare 源码：`sources/repos/<id>.git`，不进入主仓库。
- Ubuntu WSL 工作目录：`~/work/KVMPanel/upstream/<id>`。
- Linux 构建输出：`~/work/KVMPanel/build/<component>`；版本化产物归档到主仓库 `artifacts/` 并另记 manifest。

本次检查发现已有名为 `Ubuntu` 的 WSL 发行版，用户 `linro`，Git 与 Python 3 可用。这不是完整编译依赖已经安装的证明。

初始化检查确认 `make`、GCC/G++、bison、flex、dtc、pkg-config、cpio 等尚未安装。因此本次只完成源码和工作目录准备，**尚未达到可编译环境**。精确检查见 `provenance/host-environment-20261120.json`。

## 获取与核对源码

在 Windows 项目根目录执行：

```powershell
python tools/sources.py fetch
python tools/sources.py verify --full
```

版本由 `manifests/sources.lock.json` 固定。命令校验 origin、提交、必要文件和 Git 对象完整性；不会编译或访问硬件。大体积源码和缓存被 Git 忽略。

## 创建 Linux 工作目录

```powershell
wsl -d Ubuntu -- python3 /mnt/d/MyProj/KVMPanel/tools/prepare-workspaces.py
```

工具从 D 盘本地镜像复制为独立 Git 仓库，固定在锁定提交，不自动获取新版本，不覆盖有改动的工作目录。

```sh
cd ~/work/KVMPanel/upstream/linux
git switch -c codex/kvmpanel-bringup
```

此后在自有项目目录维护配置与补丁。工作目录映射写入 `sources/workspaces.json`。

## 构建约定

- Linux 使用独立 `O=` 输出目录；内核/DT/模块来自同一配置和提交。
- Buildroot 使用：`BR2_EXTERNAL=/mnt/d/MyProj/KVMPanel/software/os/buildroot`。
- 当前没有板级 defconfig，不能直接生成已适配镜像。先完成 `docs/bringup/plan.md` 中的启动配置。
- 后续固定宿主发行版、编译工具链、依赖和 Buildroot 下载哈希；不要在仓库准备阶段自动安装整套开发依赖或执行厂家脚本。

后续准备编译环境时，在 Ubuntu 中按发行版核对并安装 build-essential、git、bc、bison、flex、libssl-dev、libelf-dev、libncurses-dev、device-tree-compiler、pkg-config、cpio、rsync、file、unzip、wget、xz-utils、python3 等依赖，再运行 Buildroot 的依赖检查。交叉工具链由受控构建流程选择，不把宿主 GCC 当作 ARM 目标编译器。
