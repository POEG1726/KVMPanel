# 本地上游源码

`repos/` 存放完整固定提交的 bare Git 仓库，浅历史、无稀疏/按需缺失对象。原始源码不进入 KVMPanel 主仓库。

版本与来源：`../manifests/sources.lock.json`。获取/核对：`python ../tools/sources.py fetch` / `verify --full`（也可从项目根目录运行）。

`donors/` 存放单文件驱动参考副本；它们只有参考用途，不是完整上游内核。

`state.json` 和 `workspaces.json` 为可再生成的本机状态，Git 忽略。初始化时的验证证据另存 `provenance/`。
